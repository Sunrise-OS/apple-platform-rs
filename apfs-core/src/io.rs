// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `no_std` compatible I/O primitives.
//!
//! This crate builds on stable Rust, so it can't use `core::io`. It uses the
//! [embedded_io] traits instead. This module defines the error type shared by
//! the crate's readers and a type-erased [FilesystemReader] used to open
//! containers from any `Read + Seek` source.

pub use embedded_io::{ErrorKind, ErrorType, Read, ReadExactError, Seek, SeekFrom, Write};

use alloc::boxed::Box;
use core::fmt::{Debug, Display, Formatter};

/// An I/O error produced by this crate's readers or by an underlying device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoError {
    /// The device or stream ended before the requested bytes were read.
    UnexpectedEof,
    /// An error reported by the underlying device.
    Device(ErrorKind),
    /// The requested operation isn't implemented.
    Unsupported(&'static str),
}

impl Display for IoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("unexpected end of data"),
            Self::Device(kind) => write!(f, "device error: {kind:?}"),
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
        }
    }
}

impl core::error::Error for IoError {}

impl embedded_io::Error for IoError {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::UnexpectedEof => ErrorKind::InvalidData,
            Self::Device(kind) => *kind,
            Self::Unsupported(_) => ErrorKind::Unsupported,
        }
    }
}

impl<E: embedded_io::Error> From<ReadExactError<E>> for IoError {
    fn from(value: ReadExactError<E>) -> Self {
        match value {
            ReadExactError::UnexpectedEof => Self::UnexpectedEof,
            ReadExactError::Other(e) => Self::Device(e.kind()),
        }
    }
}

#[cfg(feature = "std")]
impl From<IoError> for std::io::Error {
    fn from(value: IoError) -> Self {
        match value {
            IoError::UnexpectedEof => std::io::ErrorKind::UnexpectedEof.into(),
            IoError::Device(kind) => std::io::Error::from(std::io::ErrorKind::from(kind)),
            IoError::Unsupported(what) => {
                std::io::Error::new(std::io::ErrorKind::Unsupported, what)
            }
        }
    }
}

/// A positioned device reader. Implementations own synchronization and must
/// fill the entire buffer or return an error. Offsets are in bytes.
///
/// Unlike a shared seek cursor, reads at different offsets cannot interfere.
/// Kernel callers can use their own sleeping locks or native positioned I/O;
/// this crate never holds a spinlock across disk access.
pub trait ReadAt: Debug + Send + Sync {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), IoError>;
}

/// An object-safe, error-erased `Read + Seek` source for the host adapter.
///
/// This is implemented for every [Read] + [Seek] + [Send] + [Debug] type.
/// Under the `std` feature, wrap `std::io` objects in `FromStd`.
pub trait FilesystemReader: Debug + Send {
    /// Seek within the underlying source.
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, IoError>;

    /// Fill `buf` completely or fail.
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), IoError>;

    /// Current position from the start of the source.
    fn stream_position(&mut self) -> Result<u64, IoError> {
        self.seek(SeekFrom::Current(0))
    }
}

impl<T> FilesystemReader for T
where
    T: Debug + Send + Read + Seek,
{
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, IoError> {
        Seek::seek(self, pos).map_err(|e| IoError::Device(embedded_io::Error::kind(&e)))
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), IoError> {
        Read::read_exact(self, buf).map_err(IoError::from)
    }
}

/// Box a stream reader for the std-only `ContainerReader::new()` adapter.
pub fn boxed<R: FilesystemReader + 'static>(reader: R) -> Box<dyn FilesystemReader> {
    Box::new(reader)
}

/// A reader limited to a fixed number of bytes, analogous to `std::io::Take`.
pub struct Take<R> {
    inner: R,
    remaining: u64,
}

impl<R> Take<R> {
    pub fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
        }
    }
}

impl<R: ErrorType> ErrorType for Take<R> {
    type Error = R::Error;
}

impl<R: Read> Read for Take<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let count = core::cmp::min(buf.len() as u64, self.remaining) as usize;
        if count == 0 {
            return Ok(0);
        }
        let read = self.inner.read(&mut buf[..count])?;
        self.remaining -= read as u64;
        Ok(read)
    }
}

/// Adapts a `std::io` type to the [embedded_io] traits.
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct FromStd<T>(pub T);

#[cfg(feature = "std")]
impl<T> ErrorType for FromStd<T> {
    type Error = std::io::Error;
}

#[cfg(feature = "std")]
impl<T: std::io::Read> Read for FromStd<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.0.read(buf)
    }
}

#[cfg(feature = "std")]
impl<T: std::io::Write> Write for FromStd<T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.0.flush()
    }
}

#[cfg(feature = "std")]
impl<T: std::io::Seek> Seek for FromStd<T> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        self.0.seek(match pos {
            SeekFrom::Start(n) => std::io::SeekFrom::Start(n),
            SeekFrom::End(n) => std::io::SeekFrom::End(n),
            SeekFrom::Current(n) => std::io::SeekFrom::Current(n),
        })
    }
}

/// Adapts an [embedded_io] reader/writer to the `std::io` traits.
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct ToStd<T>(pub T);

#[cfg(feature = "std")]
impl<T: Read> std::io::Read for ToStd<T>
where
    T::Error: Send + Sync + 'static,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::from(embedded_io::Error::kind(&e)), e)
        })
    }
}

#[cfg(feature = "std")]
impl<T: Write> std::io::Write for ToStd<T>
where
    T::Error: Send + Sync + 'static,
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::from(embedded_io::Error::kind(&e)), e)
        })
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::from(embedded_io::Error::kind(&e)), e)
        })
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for IoError {
    fn from(value: std::io::Error) -> Self {
        if value.kind() == std::io::ErrorKind::UnexpectedEof {
            Self::UnexpectedEof
        } else {
            Self::Device(value.kind().into())
        }
    }
}
