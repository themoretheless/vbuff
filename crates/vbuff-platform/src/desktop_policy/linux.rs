#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinuxDesktop {
    Gnome,
    Kde,
    Sway,
    Hyprland,
    X11,
    UnknownWayland,
}

pub const fn linux_environment_note(desktop: LinuxDesktop) -> &'static str {
    match desktop {
        LinuxDesktop::Gnome => {
            "GNOME global shortcuts require a portal grant; paste stays copy-only without target proof."
        }
        LinuxDesktop::Kde => {
            "KDE shortcut and clipboard capabilities vary by Plasma version; verify the Trust capability rows."
        }
        LinuxDesktop::Sway => {
            "Sway requires compositor configuration for shortcuts; generic automatic paste is unavailable."
        }
        LinuxDesktop::Hyprland => {
            "Hyprland requires compositor configuration for shortcuts; generic automatic paste is unavailable."
        }
        LinuxDesktop::X11 => {
            "X11 supports the generic hotkey path, but foreground identity and sensitive history exclusion remain unproven."
        }
        LinuxDesktop::UnknownWayland => {
            "This Wayland compositor is unverified; use the popup or menu and expect copy-only delivery."
        }
    }
}
