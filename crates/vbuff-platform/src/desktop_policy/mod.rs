//! Desktop policy primitives that stay independent from native registration.

mod access;
mod hotkey;
mod linux;
mod permission;
mod profile;
mod theme;

pub use access::ResidentAccessMode;
pub use hotkey::{HotkeyResolution, LayoutAwareAccelerator, resolve_hotkey_conflict};
pub use linux::{LinuxDesktop, linux_environment_note};
pub use permission::{
    PermissionRepairAction, PermissionRepairKind, PermissionRepairPlan, permission_repair_plan,
};
pub use profile::{EffectiveDesktopPolicy, ManagedInstallPolicy, ProfileLocation};
pub use theme::{NativeTheme, NativeThemeState};

#[cfg(test)]
mod tests;
