use crate::rendertree::{self, hit::{DefaultHitTester, HitTester}, PlatformContext, XY};
use crate::AlloyContext;
use crate::plugins;
use alloy::impellers::DisplayListBuilder;
use flux::rquickjs::{Ctx as QuickJsContext, Function};
use std::sync::Arc;

pub fn init(qtx: QuickJsContext<'_>, platform: Arc<PlatformContext>, atx: AlloyContext) {
  let draw_fn = Function::new(qtx.clone(), move |qtx: QuickJsContext<'_>| {
    let tree = qtx.userdata::<plugins::tree::SharedRenderTree>().unwrap();
    let mut builder = DisplayListBuilder::new(None);
    rendertree::composite::composite(&mut builder, &mut tree.0.borrow_mut(), &platform);

    let (px, py) = platform.pointer_pos();
    let hit = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(px, py));
    log::debug!("[srt] hit test ({px}, {py}) -> {hit:?}");

    if let Some(dl) = builder.build() {
      atx.submit(dl).expect("Failed to submit display list");
    }
  })
  .unwrap();

  let globals = qtx.globals();
  globals.set("draw", draw_fn).unwrap();
}
