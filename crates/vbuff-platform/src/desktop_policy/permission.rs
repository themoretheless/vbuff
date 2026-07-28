#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionRepairKind {
    Accessibility,
    GlobalShortcut,
    WaylandPortal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionRepairAction {
    OpenSystemSettings,
    PrintSteps,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRepairPlan {
    pub action: PermissionRepairAction,
    pub locator: &'static str,
    pub steps: &'static str,
}

pub fn permission_repair_plan(kind: PermissionRepairKind, target_os: &str) -> PermissionRepairPlan {
    match (kind, target_os) {
        (PermissionRepairKind::Accessibility, "macos") => PermissionRepairPlan {
            action: PermissionRepairAction::OpenSystemSettings,
            locator: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            steps: "Enable vbuff under Privacy & Security > Accessibility, then rerun the self-check.",
        },
        (PermissionRepairKind::GlobalShortcut, "windows") => PermissionRepairPlan {
            action: PermissionRepairAction::PrintSteps,
            locator: "windows-global-hotkey",
            steps: "Close the conflicting application or choose one of the verified alternative shortcuts.",
        },
        (PermissionRepairKind::WaylandPortal, "linux") => PermissionRepairPlan {
            action: PermissionRepairAction::PrintSteps,
            locator: "xdg-desktop-portal",
            steps: "Confirm the desktop portal is running, request the shortcut again, and inspect Trust capability rows.",
        },
        _ => PermissionRepairPlan {
            action: PermissionRepairAction::PrintSteps,
            locator: "platform-documentation",
            steps: "Review the platform-specific capability note and rerun the permission self-check.",
        },
    }
}
