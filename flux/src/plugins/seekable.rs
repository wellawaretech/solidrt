//! A native, seekable byte source carried on a `flux:fs` `file()` object.
//!
//! `file()` attaches one of these to every file it returns. A consumer that
//! needs synchronous, seekable, `Send` bytes off the JS thread pulls it back off
//! the object and opens a reader; audio streaming is the first such consumer
//! (SDL_mixer decodes on its own thread, so its source must be sync + seekable +
//! Send, which the async body methods on `file()` cannot be).
//!
//! The opener is opaque, so the local `flux:fs` file and the lattice dev-server
//! proxy file can each attach their own backend (disk vs HTTP range requests)
//! while the consumer stays backend-agnostic. That is how streaming rides the
//! `file()` proxy override for free: whichever `file()` is installed hands out
//! the matching reader.

use std::io::{self, Read, Seek, SeekFrom};
use std::rc::Rc;

use rquickjs::class::Trace;
use rquickjs::{Class, Ctx, JsLifetime, Object};

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
/// foreign decode thread (e.g. SDL_mixer's), not the JS thread. `Box<dyn
/// SeekableRead>` implements `Read + Seek + Send`, so it feeds `alloy`'s generic
/// stream sink directly.
pub type SeekableReader = Box<dyn SeekableRead>;

/// Opens a fresh reader over the source. Fallible: the open may touch disk or
/// the network. Not `Send` itself (it runs on the JS thread when a consumer asks
/// for the source); only the reader it yields must be.
pub type SeekableOpener = Rc<dyn Fn() -> Result<SeekableReader, String>>;

/// The property key the source is stashed under on a `file()` object. Internal;
/// not part of the public `file` surface.
const KEY: &str = "__srtSeekable";

/// An opaque handle wrapping a backend-specific opener, attached to a `file()`
/// object so a native consumer can open a seekable reader without knowing
/// whether the file is local or proxied.
#[derive(Trace)]
#[rquickjs::class(rename = "SeekableSource")]
pub struct SeekableSource {
  #[qjs(skip_trace)]
  open: SeekableOpener,
}

// SeekableSource holds no `'js`-bound data (the opener is `'static`), so the
// lifetime remap is the identity. Hand-written rather than derived because the
// boxed `dyn Fn` field is not itself `JsLifetime`.
unsafe impl<'js> JsLifetime<'js> for SeekableSource {
  type Changed<'to> = SeekableSource;
}

impl SeekableSource {
  /// Attach a seekable source to a `file()` object under the internal key.
  pub fn attach<'js>(ctx: &Ctx<'js>, obj: &Object<'js>, open: SeekableOpener) -> rquickjs::Result<()> {
    let instance = Class::instance(ctx.clone(), SeekableSource { open })?;
    obj.set(KEY, instance)
  }

  /// Pull the seekable source back off a `file()`-like object and open a reader.
  /// Errors if the object carries no source (i.e. is not a `file()`), or if the
  /// open fails.
  pub fn open_from(obj: &Object<'_>) -> Result<SeekableReader, String> {
    let instance: Class<SeekableSource> = obj.get(KEY).map_err(|_| "expected a file() from flux:fs".to_string())?;
    let open = instance.borrow().open.clone();
    open()
  }
}
