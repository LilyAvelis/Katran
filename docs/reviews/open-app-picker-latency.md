# Open With picker latency · 2026-11-10

The read-only release probe found that small-file discovery spent roughly
40–60 ms in Windows handler metadata and icons. Full source hashing remains
mandatory; a synthetic 64 MiB file added roughly 35–40 ms. The GUI adds up to
one 50 ms polling interval before accepting the completed result.

## Measurement

Built CLI only with `cargo build --offline --release --bin katran --target-dir target/perf`.
A persistent `serve-stdio` process ingested synthetic reference files, requested
`ListOpenApps` twelve times per fixture, and released each returned session.
No app was invoked, desktop controlled, personal files read or clipboard accessed.
Times include stdio transport, file validation, Shell discovery, icon handling,
session creation and JSON serialization. They exclude process startup and GUI.
First request means process/extension cold, not a cold Windows filesystem cache.
Sequential local measurements are noisy and are not a controlled hardware benchmark.

| Synthetic reference | Apps / icons | Baseline warm median ms | Pixel cache warm median ms |
| --- | --- | ---: | ---: |
| 1 KiB TXT | 17 / 15 | 42.033 | 31.933 |
| 1 KiB PNG | 20 / 18 | 55.863 | 41.145 |
| 1 KiB PDF | 22 / 20 | 49.260 | 35.392 |
| 1 KiB MP4 | 19 / 17 | 56.904 | 44.692 |

Warm medians exclude each fixture's first request. Native first TXT request was
83 ms baseline and 86 ms after: this optimization benefits repeated requests,
not initial Shell startup. All handler/icon counts were unchanged.
The 64 MiB TXT baseline warm median was 78.066 ms, versus 65.650 ms after.

An internal opt-in native benchmark separated fresh enumeration/name/recommendation
from icons. Typical metadata-only samples were TXT 25–31, PNG 33–38, PDF 27–35,
and MP4 36–49 ms; with icons they were 41–53, 57–70, 46–64, and 66–91 ms.
This motivated caching only the resource resolution and pixel work. Caching only
extraction after indirect-resource resolution initially helped TXT/PDF but barely
PNG, so the final key is the exact raw Shell icon resource string plus index.
Run the read-only benchmark with:

```powershell
cargo test --offline --release --lib native_discovery_latency --target-dir target/perf -- --ignored --nocapture
```

## Contract and limits

The process-local cache retains at most 128 entries and 2 MiB of resource-key and
encoded-pixel bytes. Successful icons expire after ten seconds; failures after
two. A poisoned cache is bypassed and resource loading proceeds normally. No
COM interface, HICON, GDI object, source path capability or handler list is cached.
Resource strings are kept exact rather than case-folded or filesystem-normalized;
Windows indirect-resource syntax is meaningful and normalization can alter it.

Every discovery still enumerates current Windows handlers and gets current names
and recommendation flags. Every selected file still receives the existing full
content revision check. Invocation still re-enumerates and authorizes the selected
handler. An installed app's changed artwork may take up to ten seconds to appear;
a previously unavailable icon is retried within two seconds. Association membership
and offered-session semantics are unaffected. No shared Service field is needed.

The GUI creates at most 64 small textures only when a result arrives, retaining
those handles until the picker closes. Its actual GPU upload/frame time was not
measured because physical UI verification belongs to the owner. Lowering its
50 ms polling interval could reduce visible delay further, but this change does
not modify GUI scheduling or claim measured end-to-end visible latency.

Cache tests cover resource/index separation, negative and positive expiry,
entry/byte bounds and oversized-entry eviction. Native read-only probe succeeded
on the final release binary. Initial cache tests passed; a later API/clippy run
encountered concurrent unfinished Text integration in capture/shared modules and
must be repeated by the integrator once that work compiles.

## Integrated follow-up

The integrated GUI now requests a repaint every 16 ms while application discovery
is pending, replacing its previous 50 ms wake hint. This is a scheduling change;
the table above still measures native API time, not visible end-to-end latency.
The combined Text/picker implementation passed the full 250-test API/headless
suite and Clippy with denied warnings. Three existing/opt-in latency/native
exercises remain excluded from the ordinary suite.
