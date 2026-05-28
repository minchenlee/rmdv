use iced::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePreset {
    OneDark,
    OneLight,
    GitHubDark,
    GitHubLight,
    Solarized,
    SolarizedLight,
    GruvboxDark,
    Nord,
    Dracula,
    TokyoNight,
}

impl ThemePreset {
    pub const ALL: [ThemePreset; 10] = [
        ThemePreset::OneLight,
        ThemePreset::OneDark,
        ThemePreset::GitHubLight,
        ThemePreset::GitHubDark,
        ThemePreset::SolarizedLight,
        ThemePreset::Solarized,
        ThemePreset::GruvboxDark,
        ThemePreset::Nord,
        ThemePreset::Dracula,
        ThemePreset::TokyoNight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemePreset::OneDark => "One Dark",
            ThemePreset::OneLight => "One Light",
            ThemePreset::GitHubDark => "GitHub Dark",
            ThemePreset::GitHubLight => "GitHub Light",
            ThemePreset::Solarized => "Solarized Dark",
            ThemePreset::SolarizedLight => "Solarized Light",
            ThemePreset::GruvboxDark => "Gruvbox Dark",
            ThemePreset::Nord => "Nord",
            ThemePreset::Dracula => "Dracula",
            ThemePreset::TokyoNight => "Tokyo Night",
        }
    }

    pub fn is_dark(self) -> bool {
        matches!(
            self,
            ThemePreset::OneDark
                | ThemePreset::GitHubDark
                | ThemePreset::Solarized
                | ThemePreset::GruvboxDark
                | ThemePreset::Nord
                | ThemePreset::Dracula
                | ThemePreset::TokyoNight
        )
    }

    pub fn next(self) -> ThemePreset {
        let idx = ThemePreset::ALL
            .iter()
            .position(|t| *t == self)
            .unwrap_or(0);
        ThemePreset::ALL[(idx + 1) % ThemePreset::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub bg: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub sidebar: Color,
    pub fg: Color,
    pub muted: Color,
    pub subtle: Color,
    pub accent: Color,
    pub accent_fg: Color,
    pub code_bg: Color,
    pub code_border: Color,
    pub rule: Color,
    pub selection: Color,
    pub match_bg: Color,
    pub match_current_bg: Color,
    pub scroller: Color,
    pub scroller_hover: Color,
    pub indent_guide: Color,
    pub tree_selected_bg: Color,
    pub tree_selected_border: Color,
    pub syntax: SyntaxPalette,
}

/// Per-theme syntax highlighting colors. Maps directly to `HlStyle` variants.
/// Upstream-accurate values pulled from each theme's canonical spec — see the
/// `// Source:` comment on each preset constant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyntaxPalette {
    pub keyword: Color,
    pub type_: Color,
    pub function: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub operator: Color,
    pub constant: Color,
    pub variable: Color,
    pub punctuation: Color,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}
const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::from_rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

impl SyntaxPalette {
    // Atom One Dark — atom/one-dark-syntax
    pub const ONE_DARK: SyntaxPalette = SyntaxPalette {
        keyword: rgb(198, 120, 221),
        type_: rgb(229, 192, 123),
        function: rgb(97, 175, 239),
        string: rgb(152, 195, 121),
        number: rgb(209, 154, 102),
        comment: rgb(92, 99, 112),
        operator: rgb(86, 182, 194),
        constant: rgb(209, 154, 102),
        variable: rgb(224, 108, 117),
        punctuation: rgb(171, 178, 191),
    };

    // Atom One Light — atom/one-light-syntax
    pub const ONE_LIGHT: SyntaxPalette = SyntaxPalette {
        keyword: rgb(166, 38, 164),
        type_: rgb(193, 132, 1),
        function: rgb(64, 120, 242),
        string: rgb(80, 161, 79),
        number: rgb(152, 104, 1),
        comment: rgb(160, 161, 167),
        operator: rgb(1, 132, 188),
        constant: rgb(152, 104, 1),
        variable: rgb(228, 86, 73),
        punctuation: rgb(56, 58, 66),
    };

    // GitHub Dark — primer/primitives
    pub const GITHUB_DARK: SyntaxPalette = SyntaxPalette {
        keyword: rgb(255, 123, 114),
        type_: rgb(255, 166, 87),
        function: rgb(210, 168, 255),
        string: rgb(165, 214, 255),
        number: rgb(121, 192, 255),
        comment: rgb(139, 148, 158),
        operator: rgb(255, 123, 114),
        constant: rgb(121, 192, 255),
        variable: rgb(255, 166, 87),
        punctuation: rgb(201, 209, 217),
    };

    // GitHub Light — primer/primitives
    pub const GITHUB_LIGHT: SyntaxPalette = SyntaxPalette {
        keyword: rgb(207, 34, 46),
        type_: rgb(149, 60, 0),
        function: rgb(130, 80, 223),
        string: rgb(10, 48, 105),
        number: rgb(5, 80, 174),
        comment: rgb(106, 115, 125),
        operator: rgb(207, 34, 46),
        constant: rgb(5, 80, 174),
        variable: rgb(149, 60, 0),
        punctuation: rgb(36, 41, 47),
    };

    // Solarized Dark — ethanschoonover/solarized
    pub const SOLARIZED_DARK: SyntaxPalette = SyntaxPalette {
        keyword: rgb(133, 153, 0),   // green
        type_: rgb(181, 137, 0),     // yellow
        function: rgb(38, 139, 210), // blue
        string: rgb(42, 161, 152),   // cyan
        number: rgb(211, 54, 130),   // magenta
        comment: rgb(88, 110, 117),  // base01
        operator: rgb(203, 75, 22),  // orange
        constant: rgb(211, 54, 130),
        variable: rgb(108, 113, 196), // violet
        punctuation: rgb(147, 161, 161),
    };

    // Solarized Light — ethanschoonover/solarized
    pub const SOLARIZED_LIGHT: SyntaxPalette = SyntaxPalette {
        keyword: rgb(133, 153, 0),
        type_: rgb(181, 137, 0),
        function: rgb(38, 139, 210),
        string: rgb(42, 161, 152),
        number: rgb(211, 54, 130),
        comment: rgb(147, 161, 161),
        operator: rgb(203, 75, 22),
        constant: rgb(211, 54, 130),
        variable: rgb(108, 113, 196),
        punctuation: rgb(101, 123, 131),
    };

    // Gruvbox Dark (medium) — morhetz/gruvbox
    pub const GRUVBOX_DARK: SyntaxPalette = SyntaxPalette {
        keyword: rgb(251, 73, 52),   // red
        type_: rgb(250, 189, 47),    // yellow
        function: rgb(184, 187, 38), // green
        string: rgb(184, 187, 38),
        number: rgb(211, 134, 155),  // purple
        comment: rgb(146, 131, 116), // gray
        operator: rgb(254, 128, 25), // orange
        constant: rgb(211, 134, 155),
        variable: rgb(131, 165, 152), // aqua
        punctuation: rgb(235, 219, 178),
    };

    // Nord — nordtheme/nord (nord7..nord15)
    pub const NORD: SyntaxPalette = SyntaxPalette {
        keyword: rgb(129, 161, 193),  // nord9
        type_: rgb(143, 188, 187),    // nord7
        function: rgb(136, 192, 208), // nord8
        string: rgb(163, 190, 140),   // nord14
        number: rgb(180, 142, 173),   // nord15
        comment: rgb(97, 110, 136),   // nord3 brightened
        operator: rgb(129, 161, 193),
        constant: rgb(180, 142, 173),
        variable: rgb(216, 222, 233), // nord4
        punctuation: rgb(216, 222, 233),
    };

    // Dracula — dracula/dracula-theme
    pub const DRACULA: SyntaxPalette = SyntaxPalette {
        keyword: rgb(255, 121, 198), // pink
        type_: rgb(139, 233, 253),   // cyan
        function: rgb(80, 250, 123), // green
        string: rgb(241, 250, 140),  // yellow
        number: rgb(189, 147, 249),  // purple
        comment: rgb(98, 114, 164),  // comment
        operator: rgb(255, 121, 198),
        constant: rgb(189, 147, 249),
        variable: rgb(248, 248, 242), // fg
        punctuation: rgb(248, 248, 242),
    };

    // Tokyo Night (Storm) — folke/tokyonight.nvim
    pub const TOKYO_NIGHT: SyntaxPalette = SyntaxPalette {
        keyword: rgb(187, 154, 247),  // purple
        type_: rgb(42, 195, 222),     // cyan1
        function: rgb(122, 162, 247), // blue
        string: rgb(158, 206, 106),   // green
        number: rgb(255, 158, 100),   // orange
        comment: rgb(86, 95, 137),    // comment
        operator: rgb(137, 221, 255), // cyan
        constant: rgb(255, 158, 100),
        variable: rgb(192, 202, 245), // fg
        punctuation: rgb(169, 177, 214),
    };
}

impl Palette {
    // Atom / Zed One Dark
    pub const ONE_DARK: Palette = Palette {
        bg: rgb(40, 44, 52),
        surface: rgb(33, 37, 43),
        surface_alt: rgb(47, 52, 61),
        sidebar: rgb(33, 37, 43),
        fg: rgb(220, 223, 228),
        muted: rgb(150, 156, 167),
        subtle: rgb(100, 106, 117),
        accent: rgb(229, 160, 107),
        accent_fg: rgb(26, 18, 12),
        code_bg: rgb(36, 40, 47),
        code_border: rgba(255, 255, 255, 0.06),
        rule: rgba(255, 255, 255, 0.07),
        selection: rgba(229, 160, 107, 0.25),
        match_bg: rgba(229, 192, 123, 0.45),
        match_current_bg: rgba(229, 130, 50, 0.85),
        scroller: rgba(255, 255, 255, 0.0),
        scroller_hover: rgba(255, 255, 255, 0.22),
        indent_guide: rgba(255, 255, 255, 0.06),
        tree_selected_bg: rgba(229, 160, 107, 0.12),
        tree_selected_border: rgb(229, 160, 107),
        syntax: SyntaxPalette::ONE_DARK,
    };

    // Atom / Zed One Light
    pub const ONE_LIGHT: Palette = Palette {
        bg: rgb(250, 250, 250),
        surface: rgb(255, 255, 255),
        surface_alt: rgb(240, 240, 241),
        sidebar: rgb(245, 245, 246),
        fg: rgb(56, 58, 66),
        muted: rgb(112, 116, 124),
        subtle: rgb(160, 164, 172),
        accent: rgb(217, 119, 87),
        accent_fg: rgb(255, 255, 255),
        code_bg: rgb(244, 244, 244),
        code_border: rgba(0, 0, 0, 0.08),
        rule: rgba(0, 0, 0, 0.08),
        selection: rgba(217, 119, 87, 0.22),
        match_bg: rgba(252, 207, 80, 0.65),
        match_current_bg: rgba(252, 130, 30, 0.90),
        scroller: rgba(0, 0, 0, 0.0),
        scroller_hover: rgba(0, 0, 0, 0.30),
        indent_guide: rgba(0, 0, 0, 0.08),
        tree_selected_bg: rgba(217, 119, 87, 0.10),
        tree_selected_border: rgb(217, 119, 87),
        syntax: SyntaxPalette::ONE_LIGHT,
    };

    // GitHub Dark
    pub const GITHUB_DARK: Palette = Palette {
        bg: rgb(13, 17, 23),
        surface: rgb(22, 27, 34),
        surface_alt: rgb(33, 38, 45),
        sidebar: rgb(13, 17, 23),
        fg: rgb(201, 209, 217),
        muted: rgb(139, 148, 158),
        subtle: rgb(110, 118, 129),
        accent: rgb(253, 140, 115),
        accent_fg: rgb(13, 17, 23),
        code_bg: rgb(22, 27, 34),
        code_border: rgb(48, 54, 61),
        rule: rgb(48, 54, 61),
        selection: rgba(253, 140, 115, 0.25),
        match_bg: rgba(187, 128, 9, 0.45),
        match_current_bg: rgba(255, 140, 30, 0.85),
        scroller: rgba(255, 255, 255, 0.0),
        scroller_hover: rgba(255, 255, 255, 0.22),
        indent_guide: rgb(33, 38, 45),
        tree_selected_bg: rgba(253, 140, 115, 0.12),
        tree_selected_border: rgb(253, 140, 115),
        syntax: SyntaxPalette::GITHUB_DARK,
    };

    // GitHub Light
    pub const GITHUB_LIGHT: Palette = Palette {
        bg: rgb(255, 255, 255),
        surface: rgb(246, 248, 250),
        surface_alt: rgb(234, 238, 242),
        sidebar: rgb(246, 248, 250),
        fg: rgb(36, 41, 47),
        muted: rgb(101, 109, 118),
        subtle: rgb(140, 149, 159),
        accent: rgb(188, 76, 0),
        accent_fg: rgb(255, 255, 255),
        code_bg: rgb(246, 248, 250),
        code_border: rgb(208, 215, 222),
        rule: rgb(208, 215, 222),
        selection: rgba(188, 76, 0, 0.18),
        match_bg: rgba(252, 207, 80, 0.65),
        match_current_bg: rgba(252, 130, 30, 0.90),
        scroller: rgba(0, 0, 0, 0.0),
        scroller_hover: rgba(0, 0, 0, 0.30),
        indent_guide: rgb(208, 215, 222),
        tree_selected_bg: rgba(188, 76, 0, 0.08),
        tree_selected_border: rgb(188, 76, 0),
        syntax: SyntaxPalette::GITHUB_LIGHT,
    };

    // Solarized Dark
    pub const SOLARIZED_DARK: Palette = Palette {
        bg: rgb(0, 43, 54),
        surface: rgb(7, 54, 66),
        surface_alt: rgb(20, 67, 79),
        sidebar: rgb(0, 38, 48),
        fg: rgb(147, 161, 161),
        muted: rgb(101, 123, 131),
        subtle: rgb(88, 110, 117),
        accent: rgb(203, 75, 22),
        accent_fg: rgb(253, 246, 227),
        code_bg: rgb(7, 54, 66),
        code_border: rgba(255, 255, 255, 0.07),
        rule: rgba(255, 255, 255, 0.07),
        selection: rgba(203, 75, 22, 0.25),
        match_bg: rgba(181, 137, 0, 0.55),
        match_current_bg: rgba(203, 75, 22, 0.90),
        scroller: rgba(255, 255, 255, 0.0),
        scroller_hover: rgba(255, 255, 255, 0.22),
        indent_guide: rgba(255, 255, 255, 0.07),
        tree_selected_bg: rgba(203, 75, 22, 0.12),
        tree_selected_border: rgb(203, 75, 22),
        syntax: SyntaxPalette::SOLARIZED_DARK,
    };

    // Solarized Light
    pub const SOLARIZED_LIGHT: Palette = Palette {
        bg: rgb(253, 246, 227),
        surface: rgb(238, 232, 213),
        surface_alt: rgb(228, 222, 203),
        sidebar: rgb(245, 238, 219),
        fg: rgb(101, 123, 131),
        muted: rgb(131, 148, 150),
        subtle: rgb(147, 161, 161),
        accent: rgb(203, 75, 22),
        accent_fg: rgb(253, 246, 227),
        code_bg: rgb(238, 232, 213),
        code_border: rgba(0, 0, 0, 0.10),
        rule: rgba(0, 0, 0, 0.10),
        selection: rgba(203, 75, 22, 0.18),
        match_bg: rgba(181, 137, 0, 0.45),
        match_current_bg: rgba(203, 75, 22, 0.85),
        scroller: rgba(0, 0, 0, 0.0),
        scroller_hover: rgba(0, 0, 0, 0.30),
        indent_guide: rgba(0, 0, 0, 0.08),
        tree_selected_bg: rgba(203, 75, 22, 0.10),
        tree_selected_border: rgb(203, 75, 22),
        syntax: SyntaxPalette::SOLARIZED_LIGHT,
    };

    // Gruvbox Dark (medium) — Source: github.com/morhetz/gruvbox
    pub const GRUVBOX_DARK: Palette = Palette {
        bg: rgb(40, 40, 40),          // bg0
        surface: rgb(50, 48, 47),     // bg1
        surface_alt: rgb(60, 56, 54), // bg2
        sidebar: rgb(32, 32, 32),     // bg0_h
        fg: rgb(235, 219, 178),       // fg1
        muted: rgb(168, 153, 132),    // fg4
        subtle: rgb(124, 111, 100),   // gray
        accent: rgb(254, 128, 25),    // orange bright
        accent_fg: rgb(29, 32, 33),
        code_bg: rgb(50, 48, 47),
        code_border: rgba(255, 255, 255, 0.06),
        rule: rgba(255, 255, 255, 0.07),
        selection: rgba(254, 128, 25, 0.25),
        match_bg: rgba(250, 189, 47, 0.45),
        match_current_bg: rgba(254, 128, 25, 0.85),
        scroller: rgba(255, 255, 255, 0.0),
        scroller_hover: rgba(255, 255, 255, 0.22),
        indent_guide: rgba(255, 255, 255, 0.06),
        tree_selected_bg: rgba(254, 128, 25, 0.12),
        tree_selected_border: rgb(254, 128, 25),
        syntax: SyntaxPalette::GRUVBOX_DARK,
    };

    // Nord — Source: github.com/nordtheme/nord
    pub const NORD: Palette = Palette {
        bg: rgb(46, 52, 64),          // nord0
        surface: rgb(59, 66, 82),     // nord1
        surface_alt: rgb(67, 76, 94), // nord2
        sidebar: rgb(40, 46, 57),
        fg: rgb(216, 222, 233), // nord4
        muted: rgb(136, 146, 168),
        subtle: rgb(97, 110, 136),  // nord3 brightened
        accent: rgb(136, 192, 208), // nord8
        accent_fg: rgb(46, 52, 64),
        code_bg: rgb(59, 66, 82),
        code_border: rgba(255, 255, 255, 0.06),
        rule: rgba(255, 255, 255, 0.07),
        selection: rgba(136, 192, 208, 0.28),
        match_bg: rgba(235, 203, 139, 0.45),         // nord13
        match_current_bg: rgba(208, 135, 112, 0.85), // nord12
        scroller: rgba(255, 255, 255, 0.0),
        scroller_hover: rgba(255, 255, 255, 0.22),
        indent_guide: rgba(255, 255, 255, 0.06),
        tree_selected_bg: rgba(136, 192, 208, 0.14),
        tree_selected_border: rgb(136, 192, 208),
        syntax: SyntaxPalette::NORD,
    };

    // Dracula — Source: github.com/dracula/dracula-theme
    pub const DRACULA: Palette = Palette {
        bg: rgb(40, 42, 54),      // background
        surface: rgb(68, 71, 90), // current line
        surface_alt: rgb(80, 84, 105),
        sidebar: rgb(33, 34, 44),
        fg: rgb(248, 248, 242), // foreground
        muted: rgb(149, 154, 184),
        subtle: rgb(98, 114, 164),  // comment
        accent: rgb(189, 147, 249), // purple
        accent_fg: rgb(40, 42, 54),
        code_bg: rgb(33, 34, 44),
        code_border: rgba(255, 255, 255, 0.06),
        rule: rgba(255, 255, 255, 0.07),
        selection: rgba(189, 147, 249, 0.28),
        match_bg: rgba(241, 250, 140, 0.45),
        match_current_bg: rgba(255, 184, 108, 0.85), // orange
        scroller: rgba(255, 255, 255, 0.0),
        scroller_hover: rgba(255, 255, 255, 0.22),
        indent_guide: rgba(255, 255, 255, 0.06),
        tree_selected_bg: rgba(189, 147, 249, 0.14),
        tree_selected_border: rgb(189, 147, 249),
        syntax: SyntaxPalette::DRACULA,
    };

    // Tokyo Night (Storm) — Source: github.com/folke/tokyonight.nvim
    pub const TOKYO_NIGHT: Palette = Palette {
        bg: rgb(36, 40, 59),      // bg
        surface: rgb(41, 46, 66), // bg_highlight
        surface_alt: rgb(52, 59, 88),
        sidebar: rgb(31, 35, 53),   // bg_dark
        fg: rgb(192, 202, 245),     // fg
        muted: rgb(154, 165, 206),  // fg_dark
        subtle: rgb(86, 95, 137),   // comment
        accent: rgb(122, 162, 247), // blue
        accent_fg: rgb(36, 40, 59),
        code_bg: rgb(31, 35, 53),
        code_border: rgba(255, 255, 255, 0.06),
        rule: rgba(255, 255, 255, 0.07),
        selection: rgba(122, 162, 247, 0.28),
        match_bg: rgba(224, 175, 104, 0.45),         // yellow
        match_current_bg: rgba(255, 158, 100, 0.85), // orange
        scroller: rgba(255, 255, 255, 0.0),
        scroller_hover: rgba(255, 255, 255, 0.22),
        indent_guide: rgba(255, 255, 255, 0.06),
        tree_selected_bg: rgba(122, 162, 247, 0.14),
        tree_selected_border: rgb(122, 162, 247),
        syntax: SyntaxPalette::TOKYO_NIGHT,
    };
}

#[derive(Debug, Clone, Copy)]
pub struct Typography {
    pub body_size: f32,
    pub line_height: f32,
    pub measure_ch: u32,
    pub h1_size: f32,
    pub h2_size: f32,
    pub h3_size: f32,
    pub h4_size: f32,
    pub h5_size: f32,
    pub h6_size: f32,
    pub code_size: f32,
}

impl Typography {
    pub const DEFAULT: Typography = Typography {
        body_size: 15.5,
        line_height: 1.65,
        measure_ch: 74,
        h1_size: 30.0,
        h2_size: 24.0,
        h3_size: 20.0,
        h4_size: 17.0,
        h5_size: 15.5,
        h6_size: 14.5,
        code_size: 13.5,
    };

    /// All font sizes multiplied by `factor`. line_height and measure_ch
    /// (ratios, not point sizes) are left untouched.
    pub fn scaled(self, factor: f32) -> Typography {
        Typography {
            body_size: self.body_size * factor,
            h1_size: self.h1_size * factor,
            h2_size: self.h2_size * factor,
            h3_size: self.h3_size * factor,
            h4_size: self.h4_size * factor,
            h5_size: self.h5_size * factor,
            h6_size: self.h6_size * factor,
            code_size: self.code_size * factor,
            ..self
        }
    }
}

pub fn palette_for(preset: ThemePreset) -> Palette {
    match preset {
        ThemePreset::OneDark => Palette::ONE_DARK,
        ThemePreset::OneLight => Palette::ONE_LIGHT,
        ThemePreset::GitHubDark => Palette::GITHUB_DARK,
        ThemePreset::GitHubLight => Palette::GITHUB_LIGHT,
        ThemePreset::Solarized => Palette::SOLARIZED_DARK,
        ThemePreset::SolarizedLight => Palette::SOLARIZED_LIGHT,
        ThemePreset::GruvboxDark => Palette::GRUVBOX_DARK,
        ThemePreset::Nord => Palette::NORD,
        ThemePreset::Dracula => Palette::DRACULA,
        ThemePreset::TokyoNight => Palette::TOKYO_NIGHT,
    }
}

pub fn resolve_mode(mode: ThemeMode) -> ThemePreset {
    match mode {
        ThemeMode::Light => ThemePreset::OneLight,
        ThemeMode::Dark => ThemePreset::OneDark,
        ThemeMode::System => match dark_light::detect() {
            dark_light::Mode::Dark => ThemePreset::OneDark,
            _ => ThemePreset::OneLight,
        },
    }
}

pub fn resolve(mode: ThemeMode) -> Palette {
    palette_for(resolve_mode(mode))
}

/// Identifies the currently active theme: a built-in preset or a custom theme
/// loaded from the themes directory keyed by its slug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeId {
    Preset(ThemePreset),
    Custom(String),
}

impl ThemeId {
    pub fn preset(self) -> Option<ThemePreset> {
        match self {
            ThemeId::Preset(p) => Some(p),
            _ => None,
        }
    }

    pub fn slug(&self) -> String {
        match self {
            ThemeId::Preset(p) => preset_slug(*p).to_string(),
            ThemeId::Custom(s) => s.clone(),
        }
    }
}

pub fn preset_slug(p: ThemePreset) -> &'static str {
    match p {
        ThemePreset::OneDark => "one-dark",
        ThemePreset::OneLight => "one-light",
        ThemePreset::GitHubDark => "github-dark",
        ThemePreset::GitHubLight => "github-light",
        ThemePreset::Solarized => "solarized-dark",
        ThemePreset::SolarizedLight => "solarized-light",
        ThemePreset::GruvboxDark => "gruvbox-dark",
        ThemePreset::Nord => "nord",
        ThemePreset::Dracula => "dracula",
        ThemePreset::TokyoNight => "tokyo-night",
    }
}

pub fn preset_by_slug(slug: &str) -> Option<ThemePreset> {
    ThemePreset::ALL
        .iter()
        .copied()
        .find(|p| preset_slug(*p) == slug)
}
