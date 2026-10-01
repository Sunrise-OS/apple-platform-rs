# apfs-core

`apfs-core` implements business logic for the Apple File System (APFS).

Whereas the `apfs-types` crate defines the primitive data structures in
APFS, `apfs-core` contains the code for doing things with them, such as
reading filesystem content.

## Stable Rust and no_std

The default `std` feature enables host file/path APIs, xattr support, std I/O
adapters, and random UUID generation. Disable default features to use the core
as `no_std` + `alloc` on stable Rust:

```toml
apfs-core = { version = "0.1.0", default-features = false }
```

I/O uses `embedded-io` 0.7 traits, not nightly `core::io` or `alloc::io` APIs.
`io::FromStd` wraps std readers/writers, and `io::ToStd` wraps embedded readers
for APIs such as `std::io::copy`. These adapters are available with `std`.

The container reader has two entry points:

* `ContainerReader::new(Box::new(FromStd(stream)))` opens a host stream at its
  current position and serializes seek + read with a std mutex.
* `ContainerReader::from_read_at(Arc::new(device), byte_offset)` uses an
  `io::ReadAt` backend. This works without std; the backend owns synchronization
  and must fill the requested buffer or return an error. There is no spinlock
  around device I/O inside the crate.

Without std, use `ContainerBuilder::new(uuid)` to supply a UUID explicitly.
Host image builders can also use this to avoid nondeterministic UUIDs.
Host filesystem extraction and `std::path` helpers are unavailable without std;
B-tree, inode, directory-entry, and file-extent operations remain available.

An allocator and device backend are still needed in a kernel. This library does
not provide XNU VFS integration or make the parser safe for untrusted disks.

```sh
cargo test -p apfs-core
cargo test -p apfs-core --no-default-features
cargo check -p apfs-cli
```

Known limitations:

* Reading is implemented; writing only has preliminary container and
  space-manager primitives. There is no complete volume/B-tree/filesystem
  writer or rootfs formatter yet.
* Error handling is not robust. There need to be more granular error types
  instead of monolithic error types.
* Very little performance optimization. e.g. B-tree lookups aren't optimized.
  Performance is likely poor, especially on larger filesystems. There are no
  caches, so blocks will be frequently read from underlying I/O.
* Limited testing and security hardening.

That bullet list effectively says that this crate provides an alpha level
implementation of APFS. Use at your own risk.

Furthermore, the original author of this crate has no intention to evolve this
crate into a production level implementation of APFS. If you would like to
maintain this APFS implementation, please get in touch with Gregory Szorc.
