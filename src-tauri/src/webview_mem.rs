//! Trims WebView2 idle memory on Windows. The look of the UI is unchanged.
//!
//! Everything that touches WebView2 is `cfg(windows)`; the pure policy and the argument
//! string compile everywhere so they are unit tested on every OS.

/// Tauri's default WebView2 arguments. Setting `additional_browser_args` replaces them,
/// so they must be repeated.
const TAURI_DEFAULT_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// Extra switches that stop background work. `--disable-gpu` and `--single-process` are
/// deliberately absent: the first changes text rendering, the second is unsupported.
const MEMORY_ARGS: &str =
    "--disable-extensions --disable-background-networking --disable-component-update";

/// The complete WebView2 browser argument string (the only place it is built).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn browser_args() -> String {
    format!("{TAURI_DEFAULT_ARGS} {MEMORY_ARGS}")
}

/// WebView2 memory usage target level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// `Normal` is unused while the policy is Low-always; it is the one-line policy switch.
#[allow(dead_code)]
pub enum MemoryLevel {
    Normal,
    Low,
}

/// Whether the level must be pushed to WebView2: always the first time, then only on change.
/// Resize events fire continuously while dragging, and every call makes WebView2 trim memory.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn needs_update(last: Option<MemoryLevel>, next: MemoryLevel) -> bool {
    last != Some(next)
}

/// Memory policy. Currently Low always (measured config: 79.9 MB -> 61.1 MB idle).
/// To use Normal while the window is focused, return `Normal` when `focused && !minimized`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn memory_level(_focused: bool, _minimized: bool) -> MemoryLevel {
    MemoryLevel::Low
}

#[cfg(windows)]
pub fn apply<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>, wanted: MemoryLevel) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL,
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows_core::Interface;

    let level: COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL = match wanted {
        MemoryLevel::Low => COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        MemoryLevel::Normal => COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    let result = window.with_webview(move |webview| {
        // SAFETY: COM calls on the WebView2 controller, made on the UI thread by `with_webview`.
        unsafe {
            let core = match webview.controller().CoreWebView2() {
                Ok(core) => core,
                Err(e) => {
                    log::warn!("WebView2 memory level: no CoreWebView2: {e}");
                    return;
                }
            };
            match core.cast::<ICoreWebView2_19>() {
                Ok(core19) => {
                    if let Err(e) = core19.SetMemoryUsageTargetLevel(level) {
                        log::warn!("WebView2 memory level could not be set: {e}");
                    }
                }
                Err(_) => {
                    log::info!("WebView2 runtime lacks ICoreWebView2_19; memory level unchanged")
                }
            }
        }
    });
    if let Err(e) = result {
        log::warn!("WebView2 memory level: with_webview failed: {e}");
    }
}

/// Builds the main window (config-declared, `create: false`) and wires the memory policy.
pub fn build_main_window(app: &tauri::App) -> tauri::Result<()> {
    let config = app
        .config()
        .app
        .windows
        .first()
        .cloned()
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    #[allow(unused_mut)]
    let mut builder = tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?;
    #[cfg(windows)]
    {
        builder = builder.additional_browser_args(&browser_args());
    }
    #[allow(unused_variables)]
    let window = builder.build()?;
    #[cfg(windows)]
    {
        let last = std::sync::Mutex::new(None::<MemoryLevel>);
        let watched = window.clone();
        let refresh = move |focused: bool| {
            let minimized = watched.is_minimized().unwrap_or_else(|e| {
                log::debug!("is_minimized failed: {e}");
                false
            });
            let next = memory_level(focused, minimized);
            let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
            if needs_update(*last, next) {
                *last = Some(next);
                apply(&watched, next);
            }
        };
        refresh(true);
        let for_focus = window.clone();
        window.on_window_event(move |event| match event {
            tauri::WindowEvent::Focused(focused) => refresh(*focused),
            // Minimizing resizes the window; re-evaluate (deduplicated by `needs_update`).
            tauri::WindowEvent::Resized(_) => {
                let focused = for_focus.is_focused().unwrap_or_else(|e| {
                    log::debug!("is_focused failed: {e}");
                    true
                });
                refresh(focused)
            }
            _ => {}
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_keep_tauri_defaults_and_add_memory_switches() {
        let args = browser_args();
        assert!(args.contains("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection"));
        for flag in [
            "--disable-extensions",
            "--disable-background-networking",
            "--disable-component-update",
        ] {
            assert!(args.contains(flag), "missing {flag}");
        }
        assert!(!args.contains("--disable-gpu"));
        assert!(!args.contains("--single-process"));
    }

    #[test]
    fn needs_update_only_on_first_call_or_change() {
        assert!(needs_update(None, MemoryLevel::Low));
        assert!(needs_update(None, MemoryLevel::Normal));
        assert!(!needs_update(Some(MemoryLevel::Low), MemoryLevel::Low));
        assert!(!needs_update(
            Some(MemoryLevel::Normal),
            MemoryLevel::Normal
        ));
        assert!(needs_update(Some(MemoryLevel::Low), MemoryLevel::Normal));
        assert!(needs_update(Some(MemoryLevel::Normal), MemoryLevel::Low));
    }

    #[test]
    fn policy_is_low_in_every_state() {
        for focused in [true, false] {
            for minimized in [true, false] {
                assert_eq!(memory_level(focused, minimized), MemoryLevel::Low);
            }
        }
    }
}
