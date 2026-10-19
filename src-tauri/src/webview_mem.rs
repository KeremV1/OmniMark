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

/// Result of one attempt to push a level to WebView2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum ApplyOutcome {
    Applied,
    /// The runtime lacks `ICoreWebView2_19` (permanent): retrying is pointless.
    Unsupported,
    /// Transient failure (e.g. CoreWebView2 not ready yet): must be retried on the next event.
    Failed,
}

/// The last-applied level after an attempt: recorded for `Applied` and `Unsupported`,
/// unchanged for `Failed` so the next event retries.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn record_outcome(
    last: Option<MemoryLevel>,
    wanted: MemoryLevel,
    outcome: ApplyOutcome,
) -> Option<MemoryLevel> {
    match outcome {
        ApplyOutcome::Applied | ApplyOutcome::Unsupported => Some(wanted),
        ApplyOutcome::Failed => last,
    }
}

/// Memory policy. Currently Low always (measured config: 79.9 MB -> 61.1 MB idle).
/// To use Normal while the window is focused, return `Normal` when `focused && !minimized`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn memory_level(_focused: bool, _minimized: bool) -> MemoryLevel {
    MemoryLevel::Low
}

/// Pushes `wanted` to WebView2. `with_webview` runs the closure later on the UI thread, so the
/// outcome is reported through `on_done` (not called if the closure could not be queued).
#[cfg(windows)]
pub fn apply<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
    wanted: MemoryLevel,
    on_done: impl FnOnce(ApplyOutcome) + Send + 'static,
) {
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
        let outcome = unsafe {
            match webview.controller().CoreWebView2() {
                Err(e) => {
                    log::warn!("WebView2 memory level: no CoreWebView2: {e}");
                    ApplyOutcome::Failed
                }
                Ok(core) => match core.cast::<ICoreWebView2_19>() {
                    Ok(core19) => match core19.SetMemoryUsageTargetLevel(level) {
                        Ok(()) => ApplyOutcome::Applied,
                        Err(e) => {
                            log::warn!("WebView2 memory level could not be set: {e}");
                            ApplyOutcome::Failed
                        }
                    },
                    Err(_) => {
                        log::info!(
                            "WebView2 runtime lacks ICoreWebView2_19; memory level unchanged"
                        );
                        ApplyOutcome::Unsupported
                    }
                },
            }
        };
        on_done(outcome);
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
        let last = std::sync::Arc::new(std::sync::Mutex::new(None::<MemoryLevel>));
        let watched = window.clone();
        let refresh = move |focused: bool| {
            let minimized = watched.is_minimized().unwrap_or_else(|e| {
                log::debug!("is_minimized failed: {e}");
                false
            });
            let next = memory_level(focused, minimized);
            // Decide under the lock, but never hold it across `apply`.
            let current = *last.lock().unwrap_or_else(|e| e.into_inner());
            if needs_update(current, next) {
                let last = std::sync::Arc::clone(&last);
                apply(&watched, next, move |outcome| {
                    let mut guard = last.lock().unwrap_or_else(|e| e.into_inner());
                    *guard = record_outcome(*guard, next, outcome);
                });
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
    fn record_outcome_keeps_last_only_on_failure() {
        use ApplyOutcome::*;
        assert_eq!(
            record_outcome(None, MemoryLevel::Low, Applied),
            Some(MemoryLevel::Low)
        );
        assert_eq!(
            record_outcome(None, MemoryLevel::Low, Unsupported),
            Some(MemoryLevel::Low)
        );
        assert_eq!(record_outcome(None, MemoryLevel::Low, Failed), None);
        assert_eq!(
            record_outcome(Some(MemoryLevel::Normal), MemoryLevel::Low, Failed),
            Some(MemoryLevel::Normal)
        );
        // A failed first attempt is retried, an unsupported one is not.
        assert!(needs_update(
            record_outcome(None, MemoryLevel::Low, Failed),
            MemoryLevel::Low
        ));
        assert!(!needs_update(
            record_outcome(None, MemoryLevel::Low, Unsupported),
            MemoryLevel::Low
        ));
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
