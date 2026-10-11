use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::Ctx;

use crate::entry::well_formed_module_name;
use crate::Modules;

#[derive(Clone)]
struct Empty;

impl ModuleDef for Empty {
  fn declare(_decl: &Declarations<'_>) -> flux::rquickjs::Result<()> {
    Ok(())
  }

  fn evaluate<'js>(_ctx: &Ctx<'js>, _exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    Ok(())
  }
}

#[test]
fn add_lists_the_full_name_as_the_capability() {
  let modules = Modules::new().add("runtime:physics", Empty).add("runtime:acme/physics", Empty);
  assert_eq!(modules.capabilities(), vec!["runtime:physics", "runtime:acme/physics"]);
}

#[test]
#[should_panic(expected = "runtime:* name")]
fn add_refuses_the_platform_scheme() {
  Modules::new().add("flux:physics", Empty);
}

#[test]
#[should_panic(expected = "runtime:* name")]
fn add_refuses_a_name_without_a_scheme() {
  Modules::new().add("physics", Empty);
}

#[test]
#[should_panic(expected = "lowercase letters")]
fn add_refuses_a_malformed_name() {
  Modules::new().add("runtime:Physics", Empty);
}

#[test]
#[should_panic(expected = "registered twice")]
fn add_refuses_a_duplicate() {
  Modules::new().add("runtime:physics", Empty).add("runtime:physics", Empty);
}

#[test]
fn module_names_are_lowercase_with_at_most_one_scope() {
  assert!(well_formed_module_name("physics"));
  assert!(well_formed_module_name("acme/physics-3d_v2.1"));
  assert!(!well_formed_module_name(""), "a name is not empty");
  assert!(!well_formed_module_name("Physics"), "uppercase is out");
  assert!(!well_formed_module_name("acme/"), "a scope needs a name");
  assert!(!well_formed_module_name("/physics"), "a slash needs a scope");
  assert!(!well_formed_module_name("a/b/c"), "one slash at most");
  assert!(!well_formed_module_name("flux:physics"), "a second scheme is out");
  assert!(!well_formed_module_name("acme physics"), "no spaces");
}
