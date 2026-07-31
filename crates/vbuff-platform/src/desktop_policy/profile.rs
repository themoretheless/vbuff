use std::path::{Path, PathBuf};

use super::ResidentAccessMode;
use crate::KeyCombo;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileLocation {
    Standard,
    Portable { root: PathBuf },
}

impl ProfileLocation {
    pub fn portable_beside(executable: &Path) -> Result<Self, &'static str> {
        if !executable.is_absolute() {
            return Err("executable_path_must_be_absolute");
        }
        let root = executable
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or("executable_parent_missing")?
            .join("vbuff-data");
        Ok(Self::Portable { root })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedInstallPolicy {
    pub forced_mode: Option<ResidentAccessMode>,
    pub portable_profile_allowed: bool,
    pub locked_hotkey: Option<KeyCombo>,
}

impl Default for ManagedInstallPolicy {
    fn default() -> Self {
        Self {
            forced_mode: None,
            portable_profile_allowed: true,
            locked_hotkey: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveDesktopPolicy {
    pub mode: ResidentAccessMode,
    pub profile: ProfileLocation,
    pub hotkey: Option<KeyCombo>,
}

impl ManagedInstallPolicy {
    pub fn apply(
        &self,
        requested_mode: ResidentAccessMode,
        requested_profile: ProfileLocation,
        requested_hotkey: KeyCombo,
    ) -> EffectiveDesktopPolicy {
        let mode = self.forced_mode.unwrap_or(requested_mode);
        let profile = match requested_profile {
            ProfileLocation::Portable { .. } if !self.portable_profile_allowed => {
                ProfileLocation::Standard
            }
            profile => profile,
        };
        let hotkey = mode
            .hotkey_enabled()
            .then(|| self.locked_hotkey.clone().unwrap_or(requested_hotkey));
        EffectiveDesktopPolicy {
            mode,
            profile,
            hotkey,
        }
    }
}
