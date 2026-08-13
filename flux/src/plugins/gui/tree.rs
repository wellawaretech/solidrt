use alloy::impellers::{FontStyle, FontWeight};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Function, IntoJs, JsLifetime, Object, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use taffy::prelude::*;

use super::AlloyContext;
use crate::plugins::gui::value::PropValue;
use crate::plugins::marshal::OptArg;
use alloy::rendertree::{
  Commit, Damage, Element, EventInterest, FrameDriver, Measurable, MeasureContext, PlatformContext, Rect,
  RenderTree, Text, Window,
};

thread_local! {
  // setProperty (FFI prop write) calls since the last frame. Bumped in the
  // native setProperty handler below; read and reset each frame by the draw
  // bridge (in lattice) for the debug overlay. Lives on the single JS thread,
  // so no timing call crosses into JS.
  pub static SETPROP_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

// Marshals a JavaScript value into the engine-independent PropValue that
// rendertree setters consume. This is the FFI boundary: rquickjs types stay on
// this side of it. Errors are real JS exceptions raised while reading the
// value (e.g. a Proxy getter throwing) and propagate to the caller unchanged.
fn to_prop_value(value: &Value<'_>) -> rquickjs::Result<PropValue> {
  Ok(if value.is_null() || value.is_undefined() {
    PropValue::Null
  } else if let Some(b) = value.as_bool() {
    PropValue::Bool(b)
  } else if let Some(n) = value.as_number() {
    PropValue::Number(n)
  } else if let Some(s) = value.as_string() {
    match s.to_string() {
      Ok(text) => PropValue::Text(text),
      // A JS string with unpaired surrogates (possible from UTF-16 slicing in
      // text-editing code) is not valid UTF-8; degrade to a lossy copy instead
      // of failing the whole property write over string CONTENT.
      Err(_) => {
        let c = s.clone().to_cstring()?;
        // SAFETY: as_ptr/len delimit the engine-owned byte buffer of this
        // string, alive until `c` drops at the end of this block; the bytes
        // are read once and copied out by from_utf8_lossy.
        let bytes = unsafe { std::slice::from_raw_parts(c.as_ptr() as *const u8, c.len()) };
        PropValue::Text(String::from_utf8_lossy(bytes).into_owned())
      }
    }
  } else if let Some(arr) = value.as_array() {
    let mut items = Vec::with_capacity(arr.len());
    for i in 0..arr.len() {
      items.push(to_prop_value(&arr.get::<Value>(i)?)?);
    }
    PropValue::List(items)
  } else if value.as_function().is_some() {
    // Functions (event handlers) are bound in the JS renderer, not marshalled as
    // data; ignore any that reach here.
    PropValue::Null
  } else if let Some(obj) = value.as_object() {
    // Arrays and functions are already handled above, so this is a plain object:
    // marshal its own enumerable keys into a Map, recursing on each value.
    let entries = obj
      .props::<String, Value>()
      .map(|entry| {
        let (k, v) = entry?;
        Ok((k, to_prop_value(&v)?))
      })
      .collect::<rquickjs::Result<Vec<_>>>()?;
    PropValue::Map(entries)
  } else {
    PropValue::Null
  })
}

struct TextSize {
  width: f32,
  height: f32,
}

impl<'js> IntoJs<'js> for TextSize {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("width", self.width)?;
    obj.set("height", self.height)?;
    Ok(obj.into_value())
  }
}

// Rect lives in alloy (engine-free), so the rquickjs IntoJs conversion cannot
// be a trait impl on it (orphan rule). Wrap it locally for marshalling; the
// nested origin/size flattens to the JS `{x, y, width, height}` shape here.
struct JsBoundingBox(Rect);

impl<'js> IntoJs<'js> for JsBoundingBox {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("x", self.0.origin.x)?;
    obj.set("y", self.0.origin.y)?;
    obj.set("width", self.0.size.width)?;
    obj.set("height", self.0.size.height)?;
    Ok(obj.into_value())
  }
}

#[derive(Clone, JsLifetime)]
pub struct SharedRenderTree(#[qjs(skip_trace)] pub Rc<RefCell<RenderTree>>);

// State the `flux:rendertree` module binds, stashed in userdata by `store_state`
// before any import so the module's `evaluate` can build its exports.
#[derive(Clone, JsLifetime)]
struct RenderTreeState(#[qjs(skip_trace)] Rc<RenderTreeInner>);

struct RenderTreeInner {
  tree: Rc<RefCell<RenderTree>>,
  platform: Arc<PlatformContext>,
  alloy_cmd_tx: Sender<alloy::AlloyCommand>,
  atx: AlloyContext,
}

/// Create the shared render tree and stash the state the `flux:rendertree`
/// module binds, before any import. Also stores `SharedRenderTree`, which the
/// runner's draw bridge (`srt:render`) reads directly.
pub fn store_state(
  ctx: &Ctx<'_>,
  tree: RenderTree,
  alloy_cmd_tx: Sender<alloy::AlloyCommand>,
  platform: Arc<PlatformContext>,
  atx: AlloyContext,
) {
  let shared = SharedRenderTree(Rc::new(RefCell::new(tree)));
  ctx.store_userdata(shared.clone()).expect("store render tree");
  ctx
    .store_userdata(RenderTreeState(Rc::new(RenderTreeInner { tree: shared.0, platform, alloy_cmd_tx, atx })))
    .expect("store rendertree state");
}

/// The `flux:rendertree` module: the render-tree bridge the renderer drives to
/// build and mutate the native tree (create/insert/delete nodes, write
/// properties, query layout, measure text). Marshalling only - all domain logic
/// lives in alloy's rendertree. Displaying the built tree is the runner's
/// concern (`srt:render`), not part of this module.
pub struct RenderTreeModule;

impl ModuleDef for RenderTreeModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("createRoot")?;
    decl.declare("createNode")?;
    decl.declare("detachNode")?;
    decl.declare("destroyNode")?;
    decl.declare("insertNode")?;
    decl.declare("setProperty")?;
    decl.declare("setEventInterest")?;
    decl.declare("requestFrame")?;
    decl.declare("render")?;
    decl.declare("setTextInputActive")?;
    decl.declare("measureText")?;
    decl.declare("getBoundingBox")?;
    decl.declare("getBoundingBoxViewport")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let state = ctx.userdata::<RenderTreeState>().expect("rendertree state userdata");
    let tree = state.0.tree.clone();
    let platform = state.0.platform.clone();
    let alloy_cmd_tx = state.0.alloy_cmd_tx.clone();
    let atx = state.0.atx.clone();

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let create_root = Function::new(ctx.clone(), move |id: u64| {
      let mut tree = tree_ref.borrow_mut();
      tree.create_node(id, Window::default().with_layout());
      tree.root = Some(id);
      platform_ref.request_frame();
    })?;

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let create_node = Function::new(ctx.clone(), move |id: u64, kind: String| {
      tree_ref.borrow_mut().create_node(id, Element::from_kind(&kind));
      platform_ref.request_frame();
    })?;

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let detach_node = Function::new(ctx.clone(), move |parent_id: u64, node_id: u64| {
      tree_ref.borrow_mut().detach_node(parent_id, node_id);
      platform_ref.request_frame();
    })?;

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let destroy_node = Function::new(ctx.clone(), move |node_id: u64| {
      tree_ref.borrow_mut().destroy_node(node_id);
      platform_ref.request_frame();
    })?;

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let insert_node = Function::new(ctx.clone(), move |parent_id: u64, node_id: u64, anchor_id: OptArg<u64>| {
      tree_ref.borrow_mut().insert_node(parent_id, node_id, anchor_id.0);
      platform_ref.request_frame();
    })?;

    let tree_ref = tree.clone();
    let platform_ref = platform.clone();
    let cmd_tx = alloy_cmd_tx.clone();
    let props_atx = atx.clone();
    let set_property = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, node_id: u64, property: String, value: Value<'_>| -> rquickjs::Result<()> {
        SETPROP_COUNT.with(|c| c.set(c.get() + 1));
        let value = to_prop_value(&value)?;
        tree_ref
          .borrow_mut()
          .try_edit(node_id, |el| {
            super::properties::apply_jsx(el, &property, &value, &cmd_tx, &|id, params| {
              props_atx.set_target_params(id, params)
            })
          })
          .map_err(|msg| rquickjs::Exception::throw_message(&ctx, &msg))?;
        platform_ref.request_frame();
        Ok(())
      },
    )?;

    // Pure dispatch metadata (which pointer deliveries the node's handlers
    // want; see alloy's EventInterest): no visual change, so no frame request.
    let tree_ref = tree.clone();
    let set_event_interest = Function::new(ctx.clone(), move |ctx: Ctx<'_>, node_id: u64, bits: u32| {
      if bits & !EventInterest::KNOWN != 0 {
        return Err(rquickjs::Exception::throw_message(
          &ctx,
          &format!("setEventInterest: unknown bits 0x{:x}", bits & !EventInterest::KNOWN),
        ));
      }
      tree_ref.borrow_mut().edit(node_id, |el| {
        el.set_event_interest(EventInterest(bits));
        Damage::None
      });
      Ok(())
    })?;

    let platform_ref = platform.clone();
    let request_frame = Function::new(ctx.clone(), move || platform_ref.request_frame())?;

    // The direct draw path: put the current tree on screen now. Lets a
    // flux + alloy app render without the runner's frame loop. Runs alloy's
    // frame protocol, so when nothing changed since the last call the retained
    // display list is re-presented instead of rebuilt (fresh texture contents
    // are still sampled at the raster flush); the call itself is the demand,
    // so the driver's gate never skips it.
    let tree_ref = tree.clone();
    let render_platform = platform.clone();
    let render_atx = atx.clone();
    let render_driver = RefCell::new(FrameDriver::new());
    let render = Function::new(ctx.clone(), move || {
      let mut driver = render_driver.borrow_mut();
      let Some(frame) = driver.begin(&render_platform, true) else { return };
      match frame.commit(&mut tree_ref.borrow_mut(), &render_platform, &render_atx) {
        Err(()) => log::warn!("render: render thread unavailable, dropping frame"),
        Ok(Commit::Reused) => {}
        Ok(Commit::Build(mut b)) => {
          // The paint phase runs layout itself; the direct path has no
          // between-phase hooks to sequence.
          b.paint(&mut tree_ref.borrow_mut(), &render_platform, &render_atx);
          if b.finish(&tree_ref.borrow(), &render_platform, &render_atx).is_err() {
            log::warn!("render: render thread unavailable, dropping frame");
          }
        }
      }
    })?;

    let tree_ref = tree.clone();
    let get_bounding_box = Function::new(ctx.clone(), move |id: u64| -> Option<JsBoundingBox> {
      tree_ref.borrow().bounding_box(id).map(JsBoundingBox)
    })?;

    let tree_ref = tree.clone();
    let get_bounding_box_viewport = Function::new(ctx.clone(), move |id: u64| -> Option<JsBoundingBox> {
      tree_ref.borrow().bounding_box_viewport(id).map(JsBoundingBox)
    })?;

    let cmd_tx = alloy_cmd_tx.clone();
    let set_text_input_active = Function::new(ctx.clone(), move |active: bool, hints: OptArg<Object<'_>>| {
      let mut options = alloy::TextInputOptions::default();
      if let Some(h) = hints.0 {
        if let Ok(v) = h.get::<_, String>("type") {
          options.input_type = match v.as_str() {
            "text" => Some(alloy::TextInputType::Text),
            "name" => Some(alloy::TextInputType::Name),
            "email" => Some(alloy::TextInputType::Email),
            "username" => Some(alloy::TextInputType::Username),
            "password" => Some(alloy::TextInputType::PasswordHidden),
            "number" => Some(alloy::TextInputType::Number),
            "pin" => Some(alloy::TextInputType::NumberPasswordHidden),
            _ => None,
          };
        }
        if let Ok(v) = h.get::<_, String>("capitalize") {
          options.capitalize = match v.as_str() {
            "none" => Some(alloy::TextCapitalization::None),
            "sentences" => Some(alloy::TextCapitalization::Sentences),
            "words" => Some(alloy::TextCapitalization::Words),
            "letters" => Some(alloy::TextCapitalization::Letters),
            _ => None,
          };
        }
        if let Ok(v) = h.get::<_, bool>("autocorrect") {
          options.autocorrect = Some(v);
        }
        if let Ok(v) = h.get::<_, bool>("multiline") {
          options.multiline = Some(v);
        }
      }
      cmd_tx.send(alloy::AlloyCommand::SetTextInputActive(active, options)).ok();
    })?;

    let measure_platform = platform.clone();
    let measure_atx = atx.clone();
    let measure_text = Function::new(ctx.clone(), move |text: String, options: OptArg<Object<'_>>| -> TextSize {
      let mut node = Text::default();
      node.computed_text = text;

      if let Some(opts) = options.0 {
        if let Ok(v) = opts.get::<_, String>("fontFamily") {
          node.font_family = v;
        }
        if let Ok(v) = opts.get::<_, f64>("fontSize") {
          node.font_size = v as f32;
        }
        if let Ok(v) = opts.get::<_, String>("fontStyle") {
          node.font_style = match v.as_str() {
            "italic" => FontStyle::Italic,
            _ => FontStyle::Normal,
          };
        }
        if let Ok(v) = opts.get::<_, f64>("fontWeight") {
          node.font_weight = match v as u32 {
            100 => FontWeight::Thin,
            200 => FontWeight::ExtraLight,
            300 => FontWeight::Light,
            500 => FontWeight::Medium,
            600 => FontWeight::SemiBold,
            700 => FontWeight::Bold,
            800 => FontWeight::ExtraBold,
            900 => FontWeight::Black,
            _ => FontWeight::Regular,
          };
        }
        if let Ok(v) = opts.get::<_, f64>("maxLines") {
          node.max_lines = v as u32;
        }
      }

      let size = node.measure(&MeasureContext {
        platform: &measure_platform,
        alloy: &*measure_atx,
        known: Size { width: None, height: None },
        available: Size { width: AvailableSpace::MaxContent, height: AvailableSpace::MaxContent },
      });
      TextSize { width: size.width, height: size.height }
    })?;

    exports.export("createRoot", create_root)?;
    exports.export("createNode", create_node)?;
    exports.export("detachNode", detach_node)?;
    exports.export("destroyNode", destroy_node)?;
    exports.export("insertNode", insert_node)?;
    exports.export("setProperty", set_property)?;
    exports.export("setEventInterest", set_event_interest)?;
    exports.export("requestFrame", request_frame)?;
    exports.export("render", render)?;
    exports.export("setTextInputActive", set_text_input_active)?;
    exports.export("measureText", measure_text)?;
    exports.export("getBoundingBox", get_bounding_box)?;
    exports.export("getBoundingBoxViewport", get_bounding_box_viewport)?;
    Ok(())
  }
}
