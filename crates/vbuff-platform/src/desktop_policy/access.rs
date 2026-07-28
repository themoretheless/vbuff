#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResidentAccessMode {
    HotkeyAndMenu,
    HotkeyOnly,
    MenuOnly,
}

impl ResidentAccessMode {
    pub const fn hotkey_enabled(self) -> bool {
        matches!(self, Self::HotkeyAndMenu | Self::HotkeyOnly)
    }

    pub const fn menu_enabled(self) -> bool {
        matches!(self, Self::HotkeyAndMenu | Self::MenuOnly)
    }
}
