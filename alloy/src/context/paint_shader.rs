use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::gpu::{validate_params, validate_texture_bindings, ParamValue, TextureBinding, UniformKind, UniformTable};
use crate::raster::RasterCmd;
use crate::renderer::PaintShader;

use super::Context;

/// One paint shader as the UI side holds it: the renderer's handle, which a
/// `<shader>` element records its draw with, and the reflected uniform
/// table its params and inputs validate against.
#[derive(Clone)]
pub struct PaintShaderEntry {
  pub handle: PaintShader,
  pub uniforms: Rc<UniformTable>,
}

/// The paint shaders by id (`Context::create_paint_shader`), the UI side's
/// registry the paint walk reads handles from, like the texture registry.
/// Its own id space, separate from texture and program ids.
pub struct PaintShaderRegistry {
  entries: RefCell<HashMap<u64, PaintShaderEntry>>,
}

impl PaintShaderRegistry {
  pub(crate) fn new() -> Self {
    PaintShaderRegistry { entries: RefCell::new(HashMap::new()) }
  }

  pub fn get(&self, id: u64) -> Option<PaintShaderEntry> {
    self.entries.borrow().get(&id).cloned()
  }

  pub(crate) fn insert(&self, id: u64, entry: PaintShaderEntry) {
    self.entries.borrow_mut().insert(id, entry);
  }

  /// Give `id`'s entry the handle the raster thread linked again on a
  /// fresh context (okf/done/gpu-context-loss.md); the uniform table stays,
  /// the source being the same. An id the registry no longer knows is
  /// skipped.
  pub fn replace_handle(&self, id: u64, handle: PaintShader) {
    if let Some(entry) = self.entries.borrow_mut().get_mut(&id) {
      entry.handle = handle;
    }
  }

  pub fn remove(&self, id: u64) -> Option<PaintShaderEntry> {
    self.entries.borrow_mut().remove(&id)
  }

  /// After a context recovery: drop every entry whose id the raster thread
  /// did not link again (`restored`), warning with the id. Its handle names
  /// a program of the dead context, which a fresh program on the new
  /// context may reuse, so a build must not record it; the element's
  /// missing-handle path (warn once, draw nothing) takes over instead.
  pub fn retain_restored(&self, restored: &HashSet<u64>) {
    self.entries.borrow_mut().retain(|id, _| {
      let kept = restored.contains(id);
      if !kept {
        log::warn!("[alloy] context recovery: paint shader {id} was not linked again; its <shader> nodes draw nothing");
      }
      kept
    });
  }
}

impl Context {
  /// Link a paint shader: the fragment program `fragment_src` behind the
  /// renderer's own vertex stage, which a `<shader>` element draws as a
  /// quad straight into the frame. The fragment contract is a shader
  /// texture's (`create_shader_texture`): no `#version` line, `vUV` 0..1
  /// over the element's box with a top-left origin, `iResolution` the box
  /// in device pixels, `fragColor` premultiplied. Returns the paint shader
  /// id (its own id space); link errors surface here, synchronously. Free
  /// with `destroy_paint_shader`.
  pub fn create_paint_shader(&self, fragment_src: &str, label: Option<String>) -> Result<u64, String> {
    let id = self.next_paint_shader_id.get();
    let (handle, uniforms) =
      self.rpc(|reply| RasterCmd::CreatePaintShader { id, source: fragment_src.to_string(), label, reply })??;
    self.next_paint_shader_id.set(id + 1);
    self.paint_shaders.insert(id, PaintShaderEntry { handle, uniforms: Rc::new(uniforms) });
    Ok(id)
  }

  /// Drop a paint shader's registry entry and retire its id. A `<shader>`
  /// naming it draws nothing from the next build; a retained recording
  /// that drew it keeps drawing it, and the GL program is deleted once the
  /// last of those is gone.
  pub fn destroy_paint_shader(&self, id: u64) {
    self.paint_shaders.remove(id);
    self.send(RasterCmd::DestroyPaintShader { id });
  }

  /// The active uniforms of paint shader `id`, with an `Inactive` slot per
  /// declared uniform the compiler optimized out. The renderer's own
  /// vertex uniform is not among them: no param sets it.
  pub fn paint_shader_uniforms(&self, id: u64) -> Result<Rc<UniformTable>, String> {
    self.paint_shaders.get(id).map(|entry| entry.uniforms).ok_or_else(|| format!("paint shader {id} not found"))
  }

  /// Check a `<shader>`'s params against paint shader `id`'s uniforms by
  /// the rules every params write follows (`validate_params`): an unknown
  /// name or a wrong component count is an error, a declared but inactive
  /// name warns. The render tree does not validate; the caller runs this
  /// before setting the params on the element.
  pub fn validate_paint_params(&self, id: u64, params: &[(String, ParamValue)]) -> Result<(), String> {
    validate_params(&*self.paint_shader_uniforms(id)?, params)
  }

  /// Check a `<shader>`'s texture inputs against paint shader `id`'s
  /// uniforms (`validate_texture_bindings`): every name an active
  /// non-array sampler, a declared but inactive name warns. A paint shader
  /// binds 2D color textures only: a slot of another sampler kind is an
  /// error, and so is an id whose registry entry has no renderer handle (a
  /// sampler-only format, a cube map, a 3D or array texture), which the
  /// draw could not bind. An id the registry does not know passes: the
  /// As for the params, the caller runs this before setting the inputs.
  pub fn validate_paint_inputs(&self, id: u64, textures: &[TextureBinding]) -> Result<(), String> {
    let uniforms = self.paint_shader_uniforms(id)?;
    validate_texture_bindings(&uniforms, textures)?;
    for binding in textures {
      let Some(slot) = uniforms.get(&binding.name) else { continue };
      match slot.kind {
        UniformKind::Inactive => continue,
        UniformKind::Sampler2D => {}
        _ => {
          return Err(format!(
            "uniform '{}' is {}: a paint shader input is a sampler2D",
            binding.name,
            slot.glsl_name()
          ))
        }
      }
      // An unknown id is refused as a shader target's binding refuses it
      // (context/target.rs), not skipped: a stale id from a signal would
      // otherwise sample black with only a log line to show for it.
      let Some(texture) = self.textures.get(binding.id) else {
        return Err(format!("texture {} (sampler '{}') not found", binding.id, binding.name));
      };
      if texture.handle.is_none() {
        return Err(format!(
          "uniform '{}': texture {} is {} {}, which a paint shader cannot sample; a paint shader input is a 2D color texture",
          binding.name,
          binding.id,
          texture.format.name(),
          texture.shape.describe()
        ));
      }
    }
    Ok(())
  }
}
