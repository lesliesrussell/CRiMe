use ratatui::style::{Color, Style};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Theme {
    Dracula,
    SolarizedDark,
    SolarizedLight,
    #[default]
    TokyoNight,
}
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub fg: Color,
    pub muted: Color,
    pub accent: Color,
    pub selection: Color,
}
impl Theme {
    pub const ALL: [Self; 4] = [
        Self::TokyoNight,
        Self::Dracula,
        Self::SolarizedDark,
        Self::SolarizedLight,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Dracula => "Dracula",
            Self::SolarizedDark => "Solarized Dark",
            Self::SolarizedLight => "Solarized Light",
            Self::TokyoNight => "Tokyo Night",
        }
    }
    pub fn slug(self) -> &'static str {
        match self {
            Self::Dracula => "dracula",
            Self::SolarizedDark => "solarized-dark",
            Self::SolarizedLight => "solarized-light",
            Self::TokyoNight => "tokyo-night",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.slug() == s)
    }
    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
    pub fn palette(self) -> Palette {
        let colors = match self {
            Self::Dracula => [0x282a36, 0xf8f8f2, 0x6272a4, 0xbd93f9, 0x44475a],
            Self::SolarizedDark => [0x002b36, 0x93a1a1, 0x839496, 0x2aa198, 0x073642],
            Self::SolarizedLight => [0xfdf6e3, 0x586e75, 0x657b83, 0x268bd2, 0xeee8d5],
            Self::TokyoNight => [0x1a1b26, 0xc0caf5, 0xa9b1d6, 0x7aa2f7, 0x283457],
        };
        let [bg, fg, muted, accent, selection] =
            colors.map(|c| Color::Rgb((c >> 16) as u8, (c >> 8) as u8, c as u8));
        Palette {
            bg,
            fg,
            muted,
            accent,
            selection,
        }
    }
}
impl Palette {
    pub fn base(self) -> Style {
        Style::default().bg(self.bg).fg(self.fg)
    }
}
