# Open With application picker

Ctrl+Shift+Enter opens Katran's own application picker for the current file.
It is a separate bottom panel using the Actions palette's search and row layout;
it is not nested inside Actions. Choosing an app invokes it for this file only.
Katran does not change Windows' default association.

Working build `6ef1a34823f7` was installed on 2026-11-10. The version label remains
1.5.0 pending the owner's release decision; this is not a new published release.

The domain API discovers handlers for the selected file's recorded extension.
The OS list is bounded to 64 handlers. Results are de-duplicated, sorted with
recommended handlers first and then by name, and include a name, recommendation
flag and optional icon. Native IDs are hashes of the extension and the Shell's
private handler identity. A handler without a stable identity or usable name is
omitted. This list reflects Windows Shell's associated handlers; it does not
promise to include every installed application.

Icons are optional 32×32 RGBA pixels, base64 encoded. The native adapter reads
icons from executable/DLL resources or exact PNG paths reported by Shell. An
indirect (`@...`) icon is resolved when Windows supplies an absolute resource
path; unresolved packaged resources or unavailable icons use the UI's generic
icon fallback. The API never returns executable paths or accepts executable
arguments from clients.

## API and job flow

`ListOpenApps{id}` creates a revision-bound session and returns a token with the
original item ID, revision, name, and offered apps. A client can call it
synchronously, or submit it through `BeginJob` for Discovery-lane work and poll
the job. Invocation is a separate Foreground-lane job when submitted through
`BeginJob`; its cancellation fence runs before the Shell effect.

```json
{"api_version":1,"request_id":"discover","command":{"type":"list_open_apps","id":42}}
{"api_version":1,"request_id":"discover-job","command":{"type":"begin_job","command":{"type":"list_open_apps","id":42}}}
{"api_version":1,"request_id":"open-app","command":{"type":"begin_job","command":{"type":"open_with_app","token":"open-apps-…","app_id":"handler-id"}}}
{"api_version":1,"request_id":"fallback","command":{"type":"begin_job","command":{"type":"open_with_app","token":"open-apps-…","app_id":null}}}
{"api_version":1,"request_id":"release","command":{"type":"release_open_apps","token":"open-apps-…"}}
```

Discovery returns a result shaped like this (the RGBA payload is abbreviated):

```json
{
  "open_apps": true,
  "token": "open-apps-…",
  "id": 42,
  "revision": "ref:…",
  "name": "draft.docx",
  "apps": [{
    "id": "handler-id",
    "name": "Example Editor",
    "recommended": true,
    "icon": {"width": 32, "height": 32, "rgba_base64": "…"}
  }]
}
```

`OpenWithApp` accepts only an ID in that token's offered list. `app_id:null`
requests Windows' native Open With chooser as the explicit “Другое
приложение…” fallback. `ReleaseOpenApps` is idempotent. Sessions are in-memory,
nonpersistent, and bounded to eight; a released or evicted token returns
`open_apps_expired`. Changing the UI cursor does not retarget a token. Before
invocation, Katran checks that the item still exists at the captured revision,
then revalidates the source contents. Stale or changed items fail without
launching. Shell enumeration on invocation verifies the handler ID again; no
client-supplied program path or arguments are run.

Cached items follow the existing `open-cache` snapshot behavior: the selected
app receives an independent named file with the original extension. The
snapshot budget, retention, and explicit cleanup requirements are documented
in [tab visibility and opening](tab-visibility-and-opening.md).

## Windows owner acceptance

After GUI wiring, check in a normal installed build:

1. Select a file and press Ctrl+Shift+Enter. Confirm the standalone bottom
   picker appears, with search and app rows, and does not open/nest the Actions
   palette.
2. Search, move with Up/Down, press Enter, and confirm the highlighted app
   receives the selected file once. Confirm the picker closes after invocation.
3. Press Escape and confirm the picker closes without opening anything.
4. Choose “Другое приложение…” and confirm Windows' native chooser opens.
   Confirm neither choosing a listed app nor using the fallback changes the
   system default association.
5. Try file extensions with several handlers, an extension with no handlers,
   and a cached file. Confirm absent icons show a generic icon and cached files
   open as independent copies.

API tests use synthetic files and a fake handler provider/launcher. Physical
Windows acceptance belongs to the owner; this document does not claim that it
has been completed.

## Verification · 2026-11-10

- Full API/headless suite: 238 passed, 2 existing explicitly opt-in tests ignored.
- Clippy with denied warnings, rustfmt and source-manifest validation passed;
  all 83 installed source hashes and the executable matched.
- Headless keyboard tests cover Ctrl+Shift+Enter through the full shelf render,
  filtered Enter invocation, arrows, Escape and session/texture cleanup.
- A read-only native stdio API check used synthetic file references for PNG,
  TXT, PDF and MP4. Windows returned application names and actual icons for all
  recommended handlers, including Paint, Snipping Tool, Notepad and Media Player.
  No application was invoked in this check and no personal source was scanned.
