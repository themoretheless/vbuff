//! Native popup navigation kept separate from rendering and side effects.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PopupSurface {
    History,
    Compose,
    Trust,
    Settings,
}

impl PopupSurface {
    pub(crate) const PRIMARY: [Self; 3] = [Self::History, Self::Compose, Self::Trust];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::History => "History",
            Self::Compose => "Compose",
            Self::Trust => "Trust",
            Self::Settings => "Settings",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_navigation_keeps_settings_as_a_secondary_action() {
        assert_eq!(
            PopupSurface::PRIMARY,
            [
                PopupSurface::History,
                PopupSurface::Compose,
                PopupSurface::Trust,
            ]
        );
        assert_eq!(PopupSurface::Settings.label(), "Settings");
    }
}
