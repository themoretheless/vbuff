//! Native target capture wrapped around the cross-platform key injector.
//!
//! The picker temporarily needs keyboard focus for search and navigation. This
//! backend remembers the application that was active before the picker opened,
//! asks the desktop to restore it after the picker hides, and exposes a separate
//! confirmation step so callers never inject into an unverified destination.

use crate::{ConfirmedPasteBackend, EnigoPaste, PasteBackend, PlatformError, Result};

/// A paste backend that refuses to inject unless the original application has
/// been restored and confirmed as foreground.
pub struct ConfirmedPaste {
    injection: EnigoPaste,
    target: Option<native::Target>,
}

impl ConfirmedPaste {
    pub fn new() -> Result<Self> {
        native::verify_permission()?;
        Ok(Self {
            injection: EnigoPaste::new()?,
            target: None,
        })
    }
}

impl PasteBackend for ConfirmedPaste {
    fn sanitize_modifiers(&mut self) -> Result<()> {
        self.injection.sanitize_modifiers()
    }

    fn paste(&mut self) -> Result<()> {
        let confirmed = native::is_foreground(
            self.target
                .as_ref()
                .ok_or_else(|| PlatformError::Paste("paste target was not captured".into()))?,
        )?;
        if !confirmed {
            return Err(PlatformError::Paste(
                "captured target is not the foreground application".into(),
            ));
        }

        let result = self.injection.paste();
        self.target = None;
        result
    }
}

impl ConfirmedPasteBackend for ConfirmedPaste {
    fn clear_target(&mut self) {
        self.target = None;
    }

    fn capture_target(&mut self) -> Result<()> {
        self.target = None;
        self.target = Some(native::capture()?);
        Ok(())
    }

    fn restore_target(&mut self) -> Result<()> {
        native::restore(
            self.target
                .as_ref()
                .ok_or_else(|| PlatformError::Paste("paste target was not captured".into()))?,
        )
    }

    fn target_is_foreground(&mut self) -> Result<bool> {
        native::is_foreground(
            self.target
                .as_ref()
                .ok_or_else(|| PlatformError::Paste("paste target was not captured".into()))?,
        )
    }
}

#[cfg(target_os = "macos")]
mod native {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

    use super::{PlatformError, Result};

    pub(super) struct Target {
        application: objc2::rc::Retained<NSRunningApplication>,
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    pub(super) fn verify_permission() -> Result<()> {
        // SAFETY: AXIsProcessTrusted has no arguments and returns one Boolean.
        if unsafe { AXIsProcessTrusted() } {
            Ok(())
        } else {
            Err(PlatformError::Paste(
                "macOS Accessibility permission is not granted".into(),
            ))
        }
    }

    pub(super) fn capture() -> Result<Target> {
        let application = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .ok_or_else(|| PlatformError::Paste("no foreground application was reported".into()))?;
        if application == NSRunningApplication::currentApplication() {
            return Err(PlatformError::Paste(
                "vbuff cannot be its own paste target".into(),
            ));
        }
        if application.isTerminated() {
            return Err(PlatformError::Paste(
                "foreground application already terminated".into(),
            ));
        }
        Ok(Target { application })
    }

    pub(super) fn restore(target: &Target) -> Result<()> {
        if target.application.isTerminated() {
            return Err(PlatformError::Paste(
                "captured application terminated before paste".into(),
            ));
        }
        if target
            .application
            .activateWithOptions(NSApplicationActivationOptions::empty())
        {
            Ok(())
        } else {
            Err(PlatformError::Paste(
                "macOS rejected target application activation".into(),
            ))
        }
    }

    pub(super) fn is_foreground(target: &Target) -> Result<bool> {
        if target.application.isTerminated() {
            return Ok(false);
        }
        let frontmost = NSWorkspace::sharedWorkspace().frontmostApplication();
        Ok(frontmost
            .as_ref()
            .is_some_and(|application| application == &target.application))
    }
}

#[cfg(target_os = "windows")]
mod native {
    use std::ffi::c_void;

    use super::{PlatformError, Result};

    type Hwnd = *mut c_void;

    pub(super) struct Target {
        window: Hwnd,
    }

    // HWND values are opaque process-external identifiers. This backend stays
    // on the event-loop thread, but PasteBackend's shared contract is Send.
    unsafe impl Send for Target {}

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> Hwnd;
        fn GetWindowThreadProcessId(window: Hwnd, process_id: *mut u32) -> u32;
        fn IsWindow(window: Hwnd) -> i32;
        fn SetForegroundWindow(window: Hwnd) -> i32;
    }

    pub(super) fn verify_permission() -> Result<()> {
        Ok(())
    }

    pub(super) fn capture() -> Result<Target> {
        // SAFETY: These read-only User32 calls accept either a system HWND or a
        // valid pointer to one u32 owned by this stack frame.
        let window = unsafe { GetForegroundWindow() };
        if window.is_null() {
            return Err(PlatformError::Paste(
                "Windows reported no foreground window".into(),
            ));
        }
        let mut process_id = 0_u32;
        unsafe { GetWindowThreadProcessId(window, &mut process_id) };
        if process_id == std::process::id() {
            return Err(PlatformError::Paste(
                "vbuff cannot be its own paste target".into(),
            ));
        }
        Ok(Target { window })
    }

    pub(super) fn restore(target: &Target) -> Result<()> {
        // SAFETY: The saved HWND is checked before it is passed back to User32.
        if unsafe { IsWindow(target.window) } == 0 {
            return Err(PlatformError::Paste(
                "captured window no longer exists".into(),
            ));
        }
        if unsafe { SetForegroundWindow(target.window) } == 0 {
            return Err(PlatformError::Paste(
                "Windows rejected foreground restoration".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn is_foreground(target: &Target) -> Result<bool> {
        // SAFETY: Both calls only inspect HWND identity.
        Ok(unsafe { IsWindow(target.window) } != 0
            && unsafe { GetForegroundWindow() } == target.window)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod native {
    use super::{PlatformError, Result};

    pub(super) struct Target;

    pub(super) fn verify_permission() -> Result<()> {
        Err(PlatformError::Paste(
            "native target confirmation is unavailable on this desktop".into(),
        ))
    }

    pub(super) fn capture() -> Result<Target> {
        Err(PlatformError::Paste(
            "native target confirmation is unavailable on this desktop".into(),
        ))
    }

    pub(super) fn restore(_target: &Target) -> Result<()> {
        Err(PlatformError::Paste(
            "native target confirmation is unavailable on this desktop".into(),
        ))
    }

    pub(super) fn is_foreground(_target: &Target) -> Result<bool> {
        Ok(false)
    }
}
