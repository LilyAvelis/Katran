# avilon-windows

Explicit Windows presentation requests for hosts that own their windows. This
crate has no Katran, egui, data, asset, or global-service dependency. It does
not change OS settings or windows automatically.

```rust,no_run
use avilon_windows::material::{apply, MaterialRequest};
// On the window's owning UI thread, after a live HWND has been created:
let hwnd: isize = /* obtain from the host window */ 1;
let result = apply(hwnd, MaterialRequest::Acrylic);
if result.is_err() { /* retain an opaque fallback */ }
// Later, request normal backdrop and default corners:
let _ = apply(hwnd, MaterialRequest::None);
```

A successful result means DWM accepted the backdrop and corner requests; it
is not a guarantee of visible Acrylic. The host must expose it through a
transparent viewport, clear color and translucent panel fills. Unsupported OS
versions and invalid windows return errors. Outside Windows the result is
explicitly `Unavailable`. Corner failure while enabling triggers a backdrop
rollback, and a rollback failure is reported. Disabling restores the normal
backdrop and default corner preference, rather than a saved earlier state.
