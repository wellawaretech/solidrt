use glow::HasContext;
use std::rc::Rc;

use super::prev_buffer;

/// A vertex buffer usable as a pipeline's interleaved attribute source.
/// Shared by Rc between the raster registry and each target's mesh state,
/// like programs and pipelines, so either destruction order is safe: the GL
/// buffer is deleted when the last user is gone (see `release_buffer`).
/// Targets also record the registry id so a write re-renders every pipeline
/// drawing from it.
pub struct GpuBuffer {
  pub vbo: glow::Buffer,
  pub size: usize,
  /// Free-form debug name from the create (WebGPU's label), surfaced in the
  /// resource inventory and raster-side messages. Not unique.
  pub label: Option<String>,
}

impl GpuBuffer {
  pub fn new(gl: &glow::Context, data: &[u8], label: Option<String>) -> Result<Self, String> {
    unsafe {
      let prev = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);
      let vbo = gl.create_buffer().map_err(|e| format!("glGenBuffers failed: {e}"))?;
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
      // DYNAMIC_DRAW: buffers back per-frame geometry (e.g. sprite quads) as
      // often as static meshes, and the hint costs static users nothing.
      gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, data, glow::DYNAMIC_DRAW);
      gl.bind_buffer(glow::ARRAY_BUFFER, prev_buffer(prev));
      Ok(GpuBuffer { vbo, size: data.len(), label })
    }
  }

  pub fn write(&self, gl: &glow::Context, data: &[u8], byte_offset: usize) -> Result<(), String> {
    let end = byte_offset.checked_add(data.len()).ok_or_else(|| "offset overflow".to_string())?;
    if end > self.size {
      return Err(format!("write of {} bytes at offset {byte_offset} exceeds buffer size {}", data.len(), self.size));
    }
    unsafe {
      let prev = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
      gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, byte_offset as i32, data);
      gl.bind_buffer(glow::ARRAY_BUFFER, prev_buffer(prev));
    }
    Ok(())
  }

  /// Read back part of the buffer via glMapBufferRange (ES 3.0's only buffer
  /// readback path; glGetBufferSubData does not exist there). On-demand and
  /// rare (a dev-server query), so the map stall is acceptable.
  pub fn read(&self, gl: &glow::Context, byte_offset: usize, len: usize) -> Result<Vec<u8>, String> {
    let end = byte_offset.checked_add(len).ok_or_else(|| "offset overflow".to_string())?;
    if end > self.size {
      return Err(format!("read of {len} bytes at offset {byte_offset} exceeds buffer size {}", self.size));
    }
    if len == 0 {
      return Ok(Vec::new());
    }
    unsafe {
      let prev = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
      let ptr = gl.map_buffer_range(glow::ARRAY_BUFFER, byte_offset as i32, len as i32, glow::MAP_READ_BIT);
      let result = if ptr.is_null() {
        Err("glMapBufferRange failed".to_string())
      } else {
        let data = std::slice::from_raw_parts(ptr, len).to_vec();
        gl.unmap_buffer(glow::ARRAY_BUFFER);
        Ok(data)
      };
      gl.bind_buffer(glow::ARRAY_BUFFER, prev_buffer(prev));
      result
    }
  }

  pub fn destroy(self, gl: &glow::Context) {
    unsafe { gl.delete_buffer(self.vbo) };
  }
}

/// Drop a use of a shared vertex buffer, deleting the GL buffer when this was
/// the last one. The raster thread is the only place buffer Rcs live, so
/// try_unwrap succeeding is exactly "no registry entry and no target still
/// draws from it" (same contract as `release_program`).
pub fn release_buffer(gl: &glow::Context, buffer: Rc<GpuBuffer>) {
  if let Ok(buffer) = Rc::try_unwrap(buffer) {
    buffer.destroy(gl);
  }
}
