//! Native popup navigation kept separate from rendering and side effects.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PopupSurface {
    History,
    Compose,
    Trust,
    Settings,
}

impl PopupSurface {
    pub(crate) const PRIMARY: [Self; 2] = [Self::History, Self::Compose];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::History => "History",
            Self::Compose => "Stack",
            Self::Trust => "Privacy",
            Self::Settings => "Settings",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_navigation_keeps_utility_surfaces_secondary() {
        assert_eq!(
            PopupSurface::PRIMARY,
            [PopupSurface::History, PopupSurface::Compose]
        );
        assert_eq!(PopupSurface::Trust.label(), "Privacy");
        assert_eq!(PopupSurface::Settings.label(), "Settings");
    }
}
