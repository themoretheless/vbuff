#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTheme {
    Light,
    Dark,
    HighContrastLight,
    HighContrastDark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeThemeState {
    current: NativeTheme,
    revision: u64,
}

impl NativeThemeState {
    pub const fn new(current: NativeTheme) -> Self {
        Self {
            current,
            revision: 0,
        }
    }

    pub const fn current(self) -> NativeTheme {
        self.current
    }

    pub const fn revision(self) -> u64 {
        self.revision
    }

    pub fn observe(&mut self, theme: NativeTheme) -> bool {
        if self.current == theme {
            return false;
        }
        self.current = theme;
        self.revision = self.revision.saturating_add(1);
        true
    }
}
