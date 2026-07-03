use std::sync::mpsc::Sender;

use alloy::AlloyCommand;

use super::str_of;
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::Window;

pub fn apply(win: &mut Window, name: &str, value: &PropValue, cmd_tx: &Sender<AlloyCommand>) -> Option<Damage> {
  Some(match name {
    "title" => win.set_title(str_of(value, "title").to_string(), cmd_tx),
    "fullscreen" => win.set_fullscreen(value.as_bool().expect("fullscreen must be a boolean"), cmd_tx),
    _ => return None,
  })
}
