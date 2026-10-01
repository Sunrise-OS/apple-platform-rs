// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use apfs_core::block::{BlockRangeReader, BlockReadError, BlockReader};
use apfs_core::io::{
    ErrorKind, ErrorType, IoError, Read, ReadAt, ReadExactError, Seek, SeekFrom, Take, Write,
};
use apfs_core::read::container::ContainerReader;
use apfs_core::write::ContainerBuilder;
use apfs_types::common::PhysicalObjectIdentifierRaw;
use apfs_types::DiskStruct;
use bytes::BytesMut;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug)]
struct MemoryDevice(Vec<u8>);

impl ReadAt for MemoryDevice {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let start =
            usize::try_from(offset).map_err(|_| IoError::Device(ErrorKind::InvalidInput))?;
        let end = start
            .checked_add(buf.len())
            .ok_or(IoError::Device(ErrorKind::InvalidInput))?;
        let data = self.0.get(start..end).ok_or(IoError::UnexpectedEof)?;
        buf.copy_from_slice(data);
        Ok(())
    }
}

struct Blocks;

impl BlockReader for Blocks {
    fn block_size(&self) -> usize {
        4
    }

    fn read_block_into<N: Into<PhysicalObjectIdentifierRaw>>(
        &self,
        number: N,
        buf: &mut BytesMut,
    ) -> Result<(), BlockReadError> {
        let number = number.into();
        let data = match number.0 {
            0 => b"abcd",
            1 => b"efgh",
            _ => return Err(BlockReadError::BlockBounds(number)),
        };
        buf.clear();
        buf.extend_from_slice(data);
        Ok(())
    }
}

#[test]
fn block_reader_crosses_boundaries_and_handles_small_buffers() {
    let mut reader = BlockRangeReader::new(&Blocks, 0u64, 2);
    let mut output = Vec::new();
    let mut buf = [0; 3];
    loop {
        let count = reader.read(&mut buf).unwrap();
        if count == 0 {
            break;
        }
        output.extend_from_slice(&buf[..count]);
    }
    assert_eq!(output, b"abcdefgh");
    assert_eq!(reader.read(&mut []).unwrap(), 0);
}

#[test]
fn block_seek_replaces_partial_buffer() {
    let mut reader = BlockRangeReader::new(&Blocks, 0u64, 2);
    reader.seek(SeekFrom::Start(1)).unwrap();
    let mut buf = [0; 3];
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"bcd");
    reader.seek(SeekFrom::Start(4)).unwrap();
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"efg");
    assert!(matches!(
        reader.seek(SeekFrom::End(0)),
        Err(BlockReadError::Io(IoError::Unsupported(_)))
    ));
}

#[test]
fn exact_read_detects_truncated_range() {
    let mut reader = BlockRangeReader::new(&Blocks, 0u64, 1);
    assert!(matches!(
        reader.read_exact(&mut [0; 5]),
        Err(ReadExactError::UnexpectedEof)
    ));
    let error: IoError = ReadExactError::<IoError>::UnexpectedEof.into();
    assert_eq!(error, IoError::UnexpectedEof);
}

#[test]
fn block_read_failure_retains_its_original_error() {
    let mut reader = BlockRangeReader::new(&Blocks, 2u64, 1);
    assert!(matches!(
        reader.read(&mut [0; 1]),
        Err(BlockReadError::BlockBounds(_))
    ));
}

#[test]
fn take_limits_reads_and_stops_without_reading_underlying_source() {
    let reader = BlockRangeReader::new(&Blocks, 0u64, 2);
    let mut reader = Take::new(reader, 5);
    let mut buf = [0; 8];
    assert_eq!(reader.read(&mut buf).unwrap(), 5);
    assert_eq!(&buf[..5], b"abcde");
    assert_eq!(reader.read(&mut buf).unwrap(), 0);
}

#[derive(Debug)]
struct PartialWriter(Vec<u8>);
impl ErrorType for PartialWriter {
    type Error = IoError;
}
impl Write for PartialWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, IoError> {
        let n = buf.len().min(2);
        self.0.extend_from_slice(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

#[test]
fn embedded_write_all_handles_partial_writes() {
    let mut writer = PartialWriter(Vec::new());
    writer.write_all(b"abcdef").unwrap();
    assert_eq!(writer.0, b"abcdef");
}

fn container_image(base: usize) -> Vec<u8> {
    let raw = ContainerBuilder::new(Uuid::nil()).make_superblock();
    let mut bytes = vec![0; base + 8192];
    bytes[base..base + raw.as_bytes().len()].copy_from_slice(raw.as_bytes());
    let checksum = apfs_core::block::fletcher64(&bytes[base + 8..base + 4096]);
    bytes[base..base + 8].copy_from_slice(&checksum.to_le_bytes());
    bytes[base + 4096..].fill(0xa5);
    bytes
}

#[test]
fn positioned_io_respects_container_offset() {
    let device = Arc::new(MemoryDevice(container_image(512)));
    let reader = ContainerReader::from_read_at(device, 512).unwrap();
    assert_eq!(reader.block_size(), 4096);
    reader.get_block_validated(0u64).unwrap();
    let block = reader.get_block(1u64).unwrap();
    assert!(block.iter().all(|&b| b == 0xa5));
    assert!(matches!(
        reader.get_block(u64::MAX),
        Err(BlockReadError::BlockBounds(_))
    ));
}

#[test]
fn positioned_io_reports_short_superblock() {
    let result = ContainerReader::from_read_at(Arc::new(MemoryDevice(vec![0; 100])), 0);
    assert!(matches!(
        result,
        Err(apfs_core::error::ApfsError::BlockRead(BlockReadError::Io(
            IoError::UnexpectedEof
        )))
    ));
}

#[cfg(feature = "std")]
#[test]
fn std_adapters_work_with_streams_at_nonzero_offsets() {
    use apfs_core::io::{FromStd, ToStd};
    let mut stream = std::io::Cursor::new(container_image(512));
    std::io::Seek::seek(&mut stream, std::io::SeekFrom::Start(512)).unwrap();
    let reader = ContainerReader::new(Box::new(FromStd(stream))).unwrap();
    reader.get_block_validated(0u64).unwrap();
    assert_eq!(reader.get_block(1u64).unwrap()[0], 0xa5);

    let source = BlockRangeReader::new(&Blocks, 0u64, 2);
    let mut output = Vec::new();
    assert_eq!(std::io::copy(&mut ToStd(source), &mut output).unwrap(), 8);
    assert_eq!(output, b"abcdefgh");

    let mut writer = FromStd(Vec::new());
    writer.write_all(b"test").unwrap();
    assert_eq!(writer.0, b"test");
    let mut writer = ToStd(PartialWriter(Vec::new()));
    std::io::Write::write_all(&mut writer, b"abcdef").unwrap();
    std::io::Write::flush(&mut writer).unwrap();
    assert_eq!(writer.0 .0, b"abcdef");
    let error: std::io::Error = IoError::UnexpectedEof.into();
    assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
}
