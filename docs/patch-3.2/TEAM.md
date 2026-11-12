# Katran 3.2 — team and work boundaries

Owner: Lilia. Lead/integrator: Polly (`/root`). Source: [Katran 3.2 Files](https://app.notion.com/p/0fad96d4d10346d09c03b493b5bd3421) and owner refinement on 2026-11-12. The Notion card is a product proposal with explicit open choices, not an accepted release.

## Starting point

- Checkout started on `main` at `f1d588f`, clean at onboarding; four local commits ahead of `origin/main` (`v3.1.1` at `985e535`). Active local branch: `codex/katran-3.2`. No push, tag, release, or distributable build has been made. Targeted tests compile into the existing `target/build` cache.
- Current 3.1.1 already has theme/optical SDK adapters, paired summon motion, tab reorder, Folder separation, and pinned tags. Work from those contracts.
- API First: Polly owns shared command/dispatcher/schema/catalogue integration. Domain behavior and synthetic API verification precede UI clients. Lilia performs native Windows UI checks; agents do not operate her desktop or personal files.
- Garden profiles came from the local SQLite snapshot after its Notion refresh failed. Each `garden.py profile` was read before assignment. The separate Notion 3.2 card was fetched successfully.

## Actual Garden sessions

| Persona | Session | Model / effort | Initial area | State |
| --- | --- | --- | --- | --- |
| Appolion / Polly | `/root` | Lead | Integration, product contracts, shared API, team map | Coordinating |
| Fenna Ferrule | `/root/fenna` | `gpt-6.1-terra` / high | Tab strip, tag menu/settings and card materials | UI integration in progress |
| Mira Socket | `/root/mira` | `gpt-6.1-terra` / high | Files pointer, async add and transfer boundary | Domain/tests complete |
| Nemi Index | `/root/nemi` | `gpt-6.1-terra` / high | SQLite tag slots, Text/Links dedup; own-Clipboard marker | Marker in progress |
| Iris Vale | `/root/iris` | `gpt-6.1-sol` / low | Tag design handoff; Files capture review | Read-only review in progress |
| Kiki Click | `/root/kiki` | `gpt-6-luna` / xhigh | Synthetic acceptance matrix | Tests complete |

Four concurrent agent slots include Polly. Completed sessions are reusable for bounded followups; a completed row does not mean an active worker. No Astra invocation.

## Separate work streams

1. **Tab strip UI:** Fenna owns `src/bin/tab_bar.rs` and, if needed, neutral Avilon geometry. One shared surface and an indicator driven by current visible tab rectangles; Folder remains separate. This is an isolated UI task on existing commands.
2. **Pinned toggle and tag menu:** `.` toggles pinned-only in the current category; the existing double press of that tab's number remains an alternate route. Pinned has no menu. Only tags get a menu, following the existing `Ctrl+K` Actions design and interaction. The tag icon belongs beside sorting in the bottom information bar; icon and keyboard route open the same tag-menu state. Fenna owns shared UI files; Polly owns new command routing after the domain contract is verified.
3. **Tag settings and materials:** A separate section in the existing `Ctrl+,` settings window for slot names, colors and textures. The card material example must distinguish tag, selection and focus in Standard and Blue/glass. Iris defines states; Fenna implements the UI after Nemi/Polly settle persistence and API.
4. **Files:** New tab, visible by default. Mira owns native file/Clipboard/transfer boundaries; Nemi owns pointer identity and durable data; Polly integrates API and default-visibility migration; Fenna connects the UI after domain proof. Deleting a Files pointer does not delete the original; Folder retains its managed-file contract. `Ctrl+Tab` copies into Folder and retains the pointer; `Shift+Tab` moves and removes the pointer only after success. Pinned follows the existing copy-only transfer guard.
5. **Text/Links duplicate observation:** Separate corrective ticket. Pods currently reuse an exact existing event in `src/storage.rs`; Text/Links deliberately create new events. Establish exact-value reuse for Text and Links through the shared storage commit path while preserving existing metadata and old duplicate records. Investigate own-Clipboard publications separately; identical content is not proof of ownership. Tables require a control case, not an assumed behavior change.

## First bounded handoffs

- Fenna: tab-strip geometry and headless input checks only; no new domain behavior, menu or materials in this ticket.
- Mira: specify Files/Folder transport and a bounded own-Clipboard marker that covers text, links and file publication without changing external capture semantics.
- Nemi: specify exact-value Text/Link event reuse and Files pointer identity/migration. Keep old duplicate IDs and their metadata; do not auto-merge.
- Iris: one synthetic tag material across Text, Images, Links, Pods and Files, with selected/focus and Standard/Blue glass states. Reuse the `Ctrl+K` Actions menu pattern and current Avilon surfaces.
- Kiki: failure matrix for Files and manual Text/Link/Table copy scenarios. No automated desktop manipulation.

Kiki's P0 matrix covers pointer creation and restart without a managed duplicate, real-file copy/drag, explicit Files-to-Folder move, pointer-only deletion, unavailable source, name collision, partial batch failure, and an actual two-volume Windows move. The first synthetic API ticket is pointer add/restart/delete with a temporary source that remains intact. For the duplicate observation, a second identical external Text/Link copy must retain the same event ID after the change; Katran's own copy must add no event; externally repeated equal Tables remain separate events as a control.

## Open product decisions

- Tags are limited to pinned objects. Unpin removes that object's tag assignment. The nine tag slot definitions keep their names, colors and materials independently. Lilia confirmed there are no tagged legacy user objects requiring migration.
- Final keyboard chords for tag assignment/removal, filter reset, and existing `Ctrl+T` compatibility. `.` means pinned-only toggle, not a pinned menu; the tag menu follows `Ctrl+K`.
- Files routing for non-image inputs and return from Folder, externally renamed/removed pointers, and visibility migration for existing profiles. New profiles show Files by default.
- Materials start with color and procedural textures. Raster presets/import are outside this initial slice. No personal artwork is included by implication.

The 3.2 implementation is underway on the local branch. Domain API tests cover Files pointer add/restart/delete, copy/move and failure edges; tag slots, assignment/unpin and filtering; Text/Links exact dedup; tab visibility and pinned toggle. No GitHub push, release, tag or native owner acceptance has occurred.
