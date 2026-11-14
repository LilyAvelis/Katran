# avilon-actions

Pure Rust shortcut normalization and assignment policy, extracted from Katran.
Hosts supply action definitions, reserved-chord rules, override storage and all
input, UI and dispatch behavior. The crate does not read or write preferences.

An absent override uses the definition's default; `None` explicitly unbinds it.
Catalogues must have unique, nonempty action IDs and canonical defaults (or an
empty default). `validate_catalogue` checks these constraints without rejecting
context-dependent default overlaps. `plan_set` and `validate_overrides` validate
automatically; call it explicitly before using the infallible `binding` lookup.
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

## Palette and visible-list navigation

`palette::Palette` supplies multiword, case-insensitive command filtering,
selection reset on query change, clamping and wraparound. Hosts supply an
ordered catalogue, scroll the selected row, execute the chosen ID and keep
their own icons and colors. `navigation::ItemCursor<T>` holds a stable item ID,
enters the first visible item, moves across the current visible list and clears
an ID absent after category changes. Hosts decide which keys invoke these
operations and whether opening an item changes application state. These pure
Rust APIs were adapted from the working local Telegram Glass action palette
and category/chat navigation; Telegram Desktop's Qt code does not link Rust.
