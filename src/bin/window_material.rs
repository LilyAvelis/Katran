//! Optional native backdrop for the main window. A successful DWM request is
//! not proof that the renderer exposes the material: the caller must also use
//! an alpha-capable viewport, clear color and translucent panel fills.

/// Request Windows 11 Desktop Acrylic or restore the normal DWM backdrop.
/// Call on the window's owning UI thread after HWND creation. The caller owns
/// presentation fallback and must keep an opaque theme when this fails.
#[cfg(windows)]
pub(super) fn apply(hwnd: isize, enabled: bool) -> Result<(), String> {
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
            DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT, DWMWCP_ROUND, DwmSetWindowAttribute,
        },
        UI::WindowsAndMessaging::IsWindow,
    };

    let window = HWND(hwnd as *mut _);
    if hwnd == 0 || !unsafe { IsWindow(Some(window)) }.as_bool() {
        return Err("Окно Katran недоступно для оформления".into());
    }
    let backdrop = if enabled {
        DWMSBT_TRANSIENTWINDOW
    } else {
        DWMSBT_NONE
    };
    let corner = if enabled {
        DWMWCP_ROUND
    } else {
        DWMWCP_DEFAULT
    };
    unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_SYSTEMBACKDROP_TYPE,
            std::ptr::from_ref(&backdrop).cast(),
            std::mem::size_of_val(&backdrop) as u32,
        )
        .map_err(|error| format!("Системный Acrylic недоступен: {error}"))?;
        if let Err(error) = DwmSetWindowAttribute(
            window,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::from_ref(&corner).cast(),
            std::mem::size_of_val(&corner) as u32,
        ) {
            if enabled {
                let none = DWMSBT_NONE;
                let rollback = DwmSetWindowAttribute(
                    window,
                    DWMWA_SYSTEMBACKDROP_TYPE,
                    std::ptr::from_ref(&none).cast(),
                    std::mem::size_of_val(&none) as u32,
                );
                return Err(match rollback {
                    Ok(()) => format!("Не удалось настроить скругление окна: {error}"),
                    Err(rollback) => format!(
                        "Не удалось настроить скругление окна: {error}; откат материала не удался: {rollback}"
                    ),
                });
            }
            return Err(format!("Не удалось восстановить скругление окна: {error}"));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn apply(_: isize, _: bool) -> Result<(), String> {
    Err("Системный Acrylic доступен только в Windows".into())
}
