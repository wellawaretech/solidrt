use crate::gpu::{ParamValue, SamplerFilter, SamplerWrap, TextureBinding};
use crate::renderer::prelude::*;
use crate::renderer::{
  ClipOperation, DisplayListBuilder, Point, Rect, RoundingRadii, ShaderInput, ShaderParam, Size, TextureSampling,
  TextureWrap,
};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Damage;
use crate::rendertree::{
  base_style, replaced_size, Bounded, BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext,
};
use std::cell::Cell;
use std::sync::Arc;
use taffy::{Display, Style};

/// A paint shader drawn as a quad over the element's box, straight into the
/// frame (`Context::create_paint_shader`): no offscreen texture, under the
/// frame's transform and clips. The element holds ids and values only; the
/// build resolves the shader and its inputs through the registries, and
/// the caller validates params and inputs against the shader's uniforms
/// before it sets them (`Context::validate_paint_params`,
/// `validate_paint_inputs`), since the tree does not.
#[derive(Clone, Debug, Default)]
pub struct Shader {
  /// The paint shader id; None draws nothing.
  pub shader_id: Option<u64>,
  /// The params, converted once on write and shared with every recording
  /// that draws them.
  pub params: Arc<[ShaderParam]>,
  /// The sampler2D inputs: the uniform name, the texture id and its
  /// per-binding sampling override.
  pub textures: Vec<TextureBinding>,
  /// Corner radii [top-left, top-right, bottom-right, bottom-left], CSS
  /// border-radius order, like Rectangle's: a rounded clip around the draw.
  pub radius: Option<[f32; 4]>,
  // Paint-only geometry for the detached (`d-shader`) form, which has no
  // layout Style to draw width/height/position from. See Rectangle's x/y/w/h.
  pub x: Option<f32>,
  pub y: Option<f32>,
  pub w: Option<f32>,
  pub h: Option<f32>,
  // The last shader id the build found missing from the registry and
  // warned about, so a missing id warns once, not once per build.
  warned: Cell<Option<u64>>,
}

impl Shader {
  fn rect(&self, fallback: Size) -> Rect {
    Rect::new(
      Point::new(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0)),
      Size::new(self.w.unwrap_or(fallback.width), self.h.unwrap_or(fallback.height)),
    )
  }

  /// The texture ids the inputs sample.
  pub fn input_ids(&self) -> impl Iterator<Item = u64> + '_ {
    self.textures.iter().map(|b| b.id)
  }
}

impl Buildable for Shader {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let Some(id) = self.shader_id else {
      return;
    };
    let Some(entry) = ctx.alloy.paint_shaders.get(id) else {
      if self.warned.get() != Some(id) {
        self.warned.set(Some(id));
        log::warn!("[shader] build: paint shader {id} not in registry; nothing drawn");
      }
      return;
    };
    // Rebuilt per build: a few entries, and the registry's handles are what
    // a context recovery replaces.
    let inputs: Arc<[ShaderInput]> = self
      .textures
      .iter()
      .filter_map(|binding| {
        let Some(texture) = ctx.alloy.textures.get(binding.id) else {
          log::warn!("[shader] build: input '{}' texture {} not in registry; skipped", binding.name, binding.id);
          return None;
        };
        let Some(handle) = &texture.handle else {
          log::warn!(
            "[shader] build: input '{}' texture {} is sampler-only ({} {}); skipped",
            binding.name,
            binding.id,
            texture.format.name(),
            texture.shape.describe()
          );
          return None;
        };
        let sampler = texture.sampler().overridden(&binding.sampler);
        let sampling = match sampler.filter {
          SamplerFilter::Linear => TextureSampling::Linear,
          SamplerFilter::Nearest => TextureSampling::Nearest,
        };
        let wrap = match sampler.wrap {
          SamplerWrap::Clamp => TextureWrap::Clamp,
          SamplerWrap::Repeat => TextureWrap::Repeat,
        };
        Some(ShaderInput { name: binding.name.clone(), texture: handle.clone(), sampling, wrap })
      })
      .collect();
    let rect = self.rect(ctx.size);
    crate::rendertree::counters::note_draw();
    match self.radius {
      Some(radii) => {
        builder.save();
        builder.clip_rounded_rect(&rect, &RoundingRadii::circular(radii), ClipOperation::Intersect);
        builder.draw_shader(&rect, &entry.handle, &self.params, &inputs);
        builder.restore();
      }
      None => {
        builder.draw_shader(&rect, &entry.handle, &self.params, &inputs);
      }
    }
  }
}

impl Hittable for Shader {
  fn is_in_bounds(&self, point: Point, ctx: &HitContext) -> bool {
    let rect = self.rect(ctx.size);
    let (x, y, w, h) = (rect.origin.x, rect.origin.y, rect.size.width, rect.size.height);
    point.x >= x && point.x < x + w && point.y >= y && point.y < y + h
  }
}

impl Bounded for Shader {
  fn local_bounds(&self, fallback: Size) -> Rect {
    self.rect(fallback)
  }
}

// No intrinsic size: a shader is as big as its box.
impl Measurable for Shader {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    replaced_size(ctx.known, Size::zero())
  }
}

impl Shader {
  // Which shader draws never changes the element box. None clears it; the
  // null-vs-number decoding happens in the binding layer.
  pub fn set_src(&mut self, id: Option<u64>) -> Damage {
    self.shader_id = id;
    Damage::Paint
  }
  // Converted to the renderer's params here, once: a build records the
  // shared slice by reference.
  pub fn set_params(&mut self, params: Vec<(String, ParamValue)>) -> Damage {
    self.params =
      params.into_iter().map(|(name, value)| ShaderParam { name, value: value.components().to_vec() }).collect();
    Damage::Paint
  }
  pub fn set_textures(&mut self, textures: Vec<TextureBinding>) -> Damage {
    self.textures = textures;
    Damage::Paint
  }
  // Paint-only: the rounding changes which pixels the draw covers, never
  // the element box.
  pub fn set_radius(&mut self, radius: Option<[f32; 4]>) -> Damage {
    self.radius = radius;
    Damage::Paint
  }

  // Detached-form geometry: painted within the parent's box (or the
  // explicit x/y/w/h given), never affects the taffy layout tree.
  pub fn set_x(&mut self, v: Option<f32>) -> Damage {
    self.x = v;
    Damage::Paint
  }
  pub fn set_y(&mut self, v: Option<f32>) -> Damage {
    self.y = v;
    Damage::Paint
  }
  pub fn set_w(&mut self, v: Option<f32>) -> Damage {
    self.w = v;
    Damage::Paint
  }
  pub fn set_h(&mut self, v: Option<f32>) -> Damage {
    self.h = v;
    Damage::Paint
  }

  pub fn initial_style() -> Style {
    // As a texture's: parent alignment applies in full.
    Style { display: Display::Block, ..base_style() }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Shader(self), Self::initial_style())
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Shader(self))
  }
}
