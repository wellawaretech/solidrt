use crate::portal::{interfaces_in, Interfaces, FILE_CHOOSER, OPEN_URI};

// The introspection of a portal that serves both interfaces, in the shape
// the portal service answers it (trimmed to what the search reads).
const BOTH: &str = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect"><arg type="s" name="xml_data" direction="out"/></method>
  </interface>
  <interface name="org.freedesktop.portal.FileChooser">
    <method name="OpenFile"><arg type="s" name="parent_window" direction="in"/></method>
    <property type="u" name="version" access="read"/>
  </interface>
  <interface name="org.freedesktop.portal.OpenURI">
    <method name="OpenURI"><arg type="s" name="uri" direction="in"/></method>
  </interface>
</node>
"#;

#[test]
fn both_interfaces_are_found() {
  assert_eq!(interfaces_in(BOTH), Interfaces { file_chooser: true, open_uri: true });
}

#[test]
fn a_missing_interface_reads_absent() {
  let no_chooser = BOTH.replace(&format!("interface name=\"{FILE_CHOOSER}\""), "interface name=\"org.example.Other\"");
  assert_eq!(interfaces_in(&no_chooser), Interfaces { file_chooser: false, open_uri: true });
  let none = "<node><interface name=\"org.freedesktop.DBus.Peer\"></interface></node>";
  assert_eq!(interfaces_in(none), Interfaces { file_chooser: false, open_uri: false });
}

#[test]
fn a_method_or_a_longer_name_is_not_the_interface() {
  // A method named like the interface, and an interface whose name only
  // starts with it, declare nothing.
  let xml = format!("<node><interface name=\"{OPEN_URI}Extra\"><method name=\"{FILE_CHOOSER}\"/></interface></node>");
  assert_eq!(interfaces_in(&xml), Interfaces { file_chooser: false, open_uri: false });
}
