//! Why the GPU device went away, on Windows: ANGLE's D3D11 device reports
//! `GetDeviceRemovedReason`, which separates a workload that hung the GPU
//! (`DXGI_ERROR_DEVICE_HUNG`) from a driver fault or a reset by another
//! process, so a field report says which. The device is reached through
//! `EGL_EXT_device_query` + `EGL_ANGLE_device_d3d` once, while the display is
//! healthy (a lost display answers no EGL query), and only its pointer is
//! kept: ANGLE owns the device for the display's life, and the one query
//! this makes runs at the loss, on the drawing thread, before anything
//! terminates the display. Taking a COM reference of our own instead left
//! the device alive past ANGLE's teardown of it, and the D3D runtime then
//! fast-failed on a corrupted internal list at the first frame of the
//! recovered context (measured; okf/done/gpu-context-loss.md).
//!
//! Elsewhere a probe knows nothing: the EGL error and the reset status are
//! the whole diagnosis. Resolved through `GlSurface::device_probe` on the
//! drawing thread with the context current; `DeviceProbe::none()` for a
//! binding without an EGL display of SDL's.

#[cfg(target_os = "windows")]
mod imp {
  use std::ffi::c_void;

  use windows::core::Interface;
  use windows::Win32::Graphics::Direct3D11::ID3D11Device;
  use windows::Win32::Graphics::Dxgi::{
    DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, DXGI_ERROR_DRIVER_INTERNAL_ERROR,
    DXGI_ERROR_INVALID_CALL,
  };

  // EGL_EXT_device_query: the display's device object.
  const EGL_DEVICE_EXT: i32 = 0x322C;
  // EGL_ANGLE_device_d3d: the ID3D11Device behind an ANGLE display.
  const EGL_D3D11_DEVICE_ANGLE: i32 = 0x33A1;

  type EglAttrib = isize;
  type QueryDisplayAttribFn = unsafe extern "C" fn(*mut c_void, i32, *mut EglAttrib) -> u32;
  type QueryDeviceAttribFn = unsafe extern "C" fn(*mut c_void, i32, *mut EglAttrib) -> u32;

  /// The D3D11 device behind the current EGL display, when ANGLE exposes
  /// one (its pointer, unowned: see the module doc); null elsewhere (a
  /// non-ANGLE EGL, no device extension).
  pub struct DeviceProbe(*mut c_void);

  impl DeviceProbe {
    /// Resolve the device now, on the drawing thread with the context
    /// current. The EGL entry points come from SDL's EGL loader, so this is
    /// the same library SDL created the display with.
    pub(crate) fn resolve() -> Self {
      // SAFETY: called with SDL's GL context current on this thread (see
      // GlSurface::device_probe); resolve_device only queries.
      let device = unsafe { resolve_device() };
      if device.is_null() {
        log::info!("[ore] D3D11 device not reachable through EGL; a loss will carry no removal reason");
      }
      DeviceProbe(device)
    }

    /// A probe that knows nothing: no removal reason, ever.
    pub fn none() -> Self {
      DeviceProbe(std::ptr::null_mut())
    }

    /// The device's removal reason, as a message fragment: None while the
    /// device is fine (or unknown), so a message only names a reason when
    /// there is one. Asked at the loss, while ANGLE still holds the device.
    pub fn removed_reason(&self) -> Option<String> {
      if self.0.is_null() {
        return None;
      }
      // SAFETY: ANGLE's device, alive for the display's life, which ends
      // only after this report (the module doc).
      let device = unsafe { <ID3D11Device as Interface>::from_raw_borrowed(&self.0) }?;
      // SAFETY: a plain query on a live device.
      let err = unsafe { device.GetDeviceRemovedReason() }.err()?;
      let code = err.code();
      let name = if code == DXGI_ERROR_DEVICE_HUNG {
        "DXGI_ERROR_DEVICE_HUNG (this process's work hung the GPU)"
      } else if code == DXGI_ERROR_DEVICE_REMOVED {
        "DXGI_ERROR_DEVICE_REMOVED (the adapter went away: driver update, reset by another process)"
      } else if code == DXGI_ERROR_DEVICE_RESET {
        "DXGI_ERROR_DEVICE_RESET (the device was reset from a bad command)"
      } else if code == DXGI_ERROR_DRIVER_INTERNAL_ERROR {
        "DXGI_ERROR_DRIVER_INTERNAL_ERROR (driver fault)"
      } else if code == DXGI_ERROR_INVALID_CALL {
        "DXGI_ERROR_INVALID_CALL"
      } else {
        "unknown"
      };
      Some(format!("D3D11 device removed: {name}, HRESULT {:#010x}", code.0 as u32))
    }
  }

  unsafe fn resolve_device() -> *mut c_void {
    // SAFETY: takes no arguments; null without a current EGL display.
    let display = unsafe { sdl3::sys::video::SDL_EGL_GetCurrentDisplay() };
    if display.is_null() {
      return std::ptr::null_mut();
    }
    // SAFETY: the names are valid C strings; SDL answers None for an entry
    // point its EGL library lacks.
    let entry_points = unsafe {
      (
        sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglQueryDisplayAttribEXT".as_ptr()),
        sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglQueryDeviceAttribEXT".as_ptr()),
      )
    };
    let (Some(query_display), Some(query_device)) = entry_points else {
      return std::ptr::null_mut();
    };
    // SAFETY: the two extension functions have exactly these signatures.
    let query_display: QueryDisplayAttribFn = unsafe { std::mem::transmute(query_display) };
    let query_device: QueryDeviceAttribFn = unsafe { std::mem::transmute(query_device) };
    let mut device: EglAttrib = 0;
    // SAFETY: a live display and a valid out-parameter.
    if unsafe { query_display(display, EGL_DEVICE_EXT, &mut device) } == 0 || device == 0 {
      return std::ptr::null_mut();
    }
    let mut d3d: EglAttrib = 0;
    // SAFETY: the display's device object and a valid out-parameter.
    if unsafe { query_device(device as *mut c_void, EGL_D3D11_DEVICE_ANGLE, &mut d3d) } == 0 || d3d == 0 {
      return std::ptr::null_mut();
    }
    d3d as *mut c_void
  }
}

#[cfg(not(target_os = "windows"))]
mod imp {
  /// No device object to ask on this platform: the EGL error and the reset
  /// status are the whole diagnosis.
  pub struct DeviceProbe;

  impl DeviceProbe {
    pub(crate) fn resolve() -> Self {
      DeviceProbe
    }

    /// A probe that knows nothing: no removal reason, ever.
    pub fn none() -> Self {
      DeviceProbe
    }

    pub fn removed_reason(&self) -> Option<String> {
      None
    }
  }
}

pub use imp::DeviceProbe;
