//! A synchronous, seekable byte source: the pull-based counterpart to the
//! forward-only push [`stream`](crate::stream) primitive.
//!
//! Where `ByteStream` models producer-driven network bodies (fetch responses,
//! request bodies), this models a consumer that pulls exactly the bytes it wants
//! and seeks freely - e.g. an audio decoder that reads and seeks on demand from
//! its own thread. Engine-free: it names only `std::io`, so any host (not just
//! flux) can produce or consume one. `fs::open_seekable` yields a local one; a
//! host can back it with anything (a dev-server range reader, an asset blob).

use std::io::{self, Read, Seek, SeekFrom};

/// Object-safe erasure of `Read + Seek + Send`. Its own `read`/`seek` forward to
/// the real traits on the concrete type, so `Box<dyn SeekableRead>` can carry any
/// backend. Declaring the methods here (rather than `Read + Seek` supertraits)
/// lets the `impl`s below give the box `Read`/`Seek` without recursing.
pub trait SeekableRead: Send {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
  fn seek(&mut self, pos: SeekFrom) -> io::Result<u64>;
}

impl<T: Read + Seek + Send> SeekableRead for T {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    Read::read(self, buf)
  }
  fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
    Seek::seek(self, pos)
  }
}

impl Read for dyn SeekableRead {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    SeekableRead::read(self, buf)
  }
}

impl Seek for dyn SeekableRead {
  fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
    SeekableRead::seek(self, pos)
  }
}

/// A synchronous, seekable, `Send` byte reader. `Send` because it is read from a
/// foreign decode thread (e.g. SDL_mixer's), not the thread that opened it.
/// `Box<dyn SeekableRead>` implements `Read + Seek + Send`, so it feeds a generic
/// `Read + Seek` sink directly.
pub type SeekableReader = Box<dyn SeekableRead>;
