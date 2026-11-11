# garden-actions

Pure Rust shortcut normalization and assignment policy, extracted from Katran.
Hosts supply action definitions, reserved-chord rules, override storage and all
input, UI and dispatch behavior. The crate does not read or write preferences.

An absent override uses the definition's default; `None` explicitly unbinds it.
`validate_overrides` normalizes stored values and checks their action IDs and
rebindability, but deliberately does not reject pre-existing conflicts or
reserved keys. `plan_set` returns a new map without changing its input. It checks
reset defaults too; replacing conflicts removes only overlapping alternatives
and cannot displace a protected action. Conflict order follows the catalogue.
`/` separates alternatives, not sequential key combinations.

See `examples/shortcuts.rs` for a neutral host example. The normalized key names
are policy values; platform-specific shortcut registration and availability are
host responsibilities. This internal, unpublished 0.1 crate uses Rust 2024.

Shortcut policy adapted from Katran; see the accompanying MIT license.
