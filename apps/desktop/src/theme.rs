//! Colours and strokes: the only place where a [`StyleKey`] gets a look.
//!
//! Conductor colours follow the usual wire identification (brown, black and
//! grey phases, blue neutral, green protective earth); a wire carrying more
//! than one conductor is drawn in a conflict colour.

use circuit_model::{Conductor, ConductorSet, MotorState};
use circuit_renderer::StyleKey;
use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Theme, Visuals,
};

/// How one item is drawn, at zoom 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Colour of lines and texts, and fill of dots.
    pub color: Color32,
    /// Stroke width in screen points.
    pub width: f32,
}

impl Look {
    /// The stroke of this look.
    #[must_use]
    pub fn stroke(self) -> Stroke {
        Stroke::new(self.width, self.color)
    }
}

const ENERGIZED: Color32 = Color32::from_rgb(240, 160, 0);
const ACTIVE: Color32 = Color32::from_rgb(40, 190, 90);
const FAULT: Color32 = Color32::from_rgb(230, 40, 40);
const SELECTED: Color32 = Color32::from_rgb(0, 170, 230);
/// The net being traced.
const TRACED: Color32 = Color32::from_rgb(255, 170, 40);
const CONFLICT: Color32 = Color32::from_rgb(220, 0, 220);
const REVERSE: Color32 = Color32::from_rgb(60, 120, 240);

/// The colour of a single conductor. Black phase `L2` is drawn in the
/// foreground colour so that it stays visible on a dark background.
fn conductor_color(conductor: Conductor, foreground: Color32) -> Color32 {
    match conductor {
        Conductor::L1 => Color32::from_rgb(160, 90, 40),
        Conductor::L2 => foreground,
        Conductor::L3 => Color32::from_gray(128),
        Conductor::N => Color32::from_rgb(40, 110, 230),
        Conductor::Pe => Color32::from_rgb(40, 170, 60),
        Conductor::DcPos => Color32::from_rgb(220, 50, 50),
        Conductor::DcNeg => Color32::from_rgb(0, 150, 210),
    }
}

/// The colour of a wire or junction with the given conductors.
fn live_color(conductors: ConductorSet, foreground: Color32, dead: Color32) -> Color32 {
    if conductors.is_empty() {
        dead
    } else {
        conductors
            .single()
            .map_or(CONFLICT, |conductor| conductor_color(conductor, foreground))
    }
}

/// The colours of the window, apart from the drawing itself (those are
/// [`look`]). Every widget of the chrome reads them from here, so a theme is
/// one table. The values are this project's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    /// Menu bar, tool bar and panels.
    pub chrome: Color32,
    /// Tab bars, the status bar and the start of the window.
    pub chrome_dark: Color32,
    /// Inside of the side panels.
    pub panel: Color32,
    /// The tab that is selected.
    pub tab_active: Color32,
    /// Buttons and fields at rest.
    pub control: Color32,
    /// Buttons under the pointer.
    pub control_hover: Color32,
    /// Lines between areas.
    pub border: Color32,
    /// The drawing area.
    pub canvas: Color32,
    /// The dots of the grid.
    pub grid: Color32,
    /// Text.
    pub text: Color32,
    /// Secondary text.
    pub text_dim: Color32,
    /// Hints and disabled text.
    pub text_faint: Color32,
    /// Icon strokes.
    pub icon: Color32,
    /// What is selected, on and focused.
    pub accent: Color32,
    /// The fill of the main button of a dialog: the accent, dark enough for
    /// [`Tokens::on_accent`] text to read at 4.5:1.
    pub accent_fill: Color32,
    /// Text over [`Tokens::accent_fill`].
    pub on_accent: Color32,
    /// An error, and what cannot be undone.
    pub danger: Color32,
    /// A message to read twice.
    pub warn: Color32,
    /// The cursor cross over the drawing.
    pub crosshair: Color32,
    /// Selecting by a box that must contain the elements.
    pub window_select: Color32,
    /// Selecting by a box that only has to touch them.
    pub crossing_select: Color32,
    /// The command line.
    pub command: Color32,
}

impl Tokens {
    /// The dark theme, the default: graphite chrome and a deep drawing area.
    pub const DARK: Self = Self {
        chrome: Color32::from_rgb(0x2b, 0x2f, 0x36),
        chrome_dark: Color32::from_rgb(0x22, 0x25, 0x2b),
        panel: Color32::from_rgb(0x2f, 0x33, 0x3b),
        tab_active: Color32::from_rgb(0x3d, 0x44, 0x50),
        control: Color32::from_rgb(0x38, 0x3d, 0x47),
        control_hover: Color32::from_rgb(0x48, 0x4f, 0x5c),
        border: Color32::from_rgb(0x1a, 0x1c, 0x21),
        canvas: Color32::from_rgb(0x1b, 0x1e, 0x24),
        grid: Color32::from_rgb(0x3a, 0x40, 0x4a),
        text: Color32::from_rgb(0xe1, 0xe5, 0xea),
        text_dim: Color32::from_rgb(0xa8, 0xb0, 0xbb),
        text_faint: Color32::from_rgb(0x7e, 0x87, 0x93),
        icon: Color32::from_rgb(0xd2, 0xd8, 0xe0),
        accent: Color32::from_rgb(0x36, 0x9c, 0xd8),
        accent_fill: Color32::from_rgb(0x28, 0x78, 0xb5),
        on_accent: Color32::WHITE,
        danger: Color32::from_rgb(0xff, 0x7b, 0x72),
        warn: Color32::from_rgb(0xf2, 0xc5, 0x6b),
        crosshair: Color32::from_rgb(0xe9, 0xec, 0xf0),
        window_select: Color32::from_rgb(0x5f, 0xa8, 0xf5),
        crossing_select: Color32::from_rgb(0x6f, 0xcf, 0x8a),
        command: Color32::from_rgb(0x25, 0x29, 0x2f),
    };

    /// The light theme: pale chrome and a white drawing area.
    pub const LIGHT: Self = Self {
        chrome: Color32::from_rgb(0xe8, 0xea, 0xee),
        chrome_dark: Color32::from_rgb(0xd9, 0xdc, 0xe2),
        panel: Color32::from_rgb(0xf1, 0xf2, 0xf5),
        tab_active: Color32::from_rgb(0xff, 0xff, 0xff),
        control: Color32::from_rgb(0xdd, 0xe1, 0xe7),
        control_hover: Color32::from_rgb(0xcb, 0xd2, 0xdc),
        border: Color32::from_rgb(0xb4, 0xba, 0xc4),
        canvas: Color32::from_rgb(0xfa, 0xfb, 0xfc),
        grid: Color32::from_rgb(0xc9, 0xce, 0xd6),
        text: Color32::from_rgb(0x1f, 0x24, 0x2b),
        text_dim: Color32::from_rgb(0x46, 0x4e, 0x5a),
        text_faint: Color32::from_rgb(0x6b, 0x73, 0x7f),
        icon: Color32::from_rgb(0x34, 0x3b, 0x46),
        accent: Color32::from_rgb(0x1f, 0x7f, 0xc0),
        accent_fill: Color32::from_rgb(0x1a, 0x6f, 0xa8),
        on_accent: Color32::WHITE,
        danger: Color32::from_rgb(0xb7, 0x1c, 0x1c),
        warn: Color32::from_rgb(0x94, 0x5c, 0x00),
        crosshair: Color32::from_rgb(0x2a, 0x2f, 0x36),
        window_select: Color32::from_rgb(0x1d, 0x6f, 0xd1),
        crossing_select: Color32::from_rgb(0x1f, 0x8f, 0x4a),
        command: Color32::from_rgb(0xf7, 0xf8, 0xfa),
    };

    /// The tokens of a dark or a light background.
    #[must_use]
    pub const fn for_mode(dark_mode: bool) -> Self {
        if dark_mode { Self::DARK } else { Self::LIGHT }
    }

    /// The tokens of the theme `ui` is drawn in.
    #[must_use]
    pub fn of(ui: &egui::Ui) -> Self {
        Self::for_mode(ui.visuals().dark_mode)
    }

    fn visuals(self, dark_mode: bool) -> Visuals {
        let mut visuals = if dark_mode {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        visuals.panel_fill = self.chrome;
        visuals.window_fill = self.panel;
        visuals.extreme_bg_color = self.canvas;
        visuals.faint_bg_color = self.chrome_dark;
        visuals.override_text_color = Some(self.text);
        visuals.window_stroke = Stroke::new(1.0, self.border);
        visuals.window_corner_radius = CornerRadius::same(window_radius());
        visuals.window_shadow = Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(if dark_mode { 110 } else { 50 }),
        };
        visuals.menu_corner_radius = CornerRadius::same(5);
        visuals.popup_shadow = Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(if dark_mode { 90 } else { 40 }),
        };
        visuals.selection.bg_fill = self.accent.gamma_multiply(0.45);
        visuals.selection.stroke = Stroke::new(1.0, self.accent);
        visuals.hyperlink_color = self.accent;
        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_fill = self.panel;
        widgets.noninteractive.weak_bg_fill = self.panel;
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text_dim);
        widgets.inactive.bg_fill = self.control;
        widgets.inactive.weak_bg_fill = self.control;
        widgets.inactive.bg_stroke = Stroke::NONE;
        widgets.inactive.fg_stroke = Stroke::new(1.0, self.text);
        widgets.hovered.bg_fill = self.control_hover;
        widgets.hovered.weak_bg_fill = self.control_hover;
        widgets.hovered.bg_stroke = Stroke::new(1.0, self.accent);
        widgets.hovered.fg_stroke = Stroke::new(1.5, self.text);
        widgets.active.bg_fill = self.accent;
        widgets.active.weak_bg_fill = self.accent;
        widgets.active.bg_stroke = Stroke::new(1.0, self.accent);
        widgets.active.fg_stroke = Stroke::new(1.5, self.text);
        widgets.open.bg_fill = self.tab_active;
        widgets.open.weak_bg_fill = self.tab_active;
        for state in [
            &mut widgets.noninteractive,
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            state.corner_radius = CornerRadius::same(4);
        }
        visuals
    }
}

/// The corner radius of windows and dialogs, in points: what each system
/// draws its own (8 on Windows, 10 on macOS, 12 on the GNOME and KDE desktops
/// of Linux).
#[must_use]
pub const fn window_radius() -> u8 {
    if cfg!(target_os = "windows") {
        8
    } else if cfg!(target_os = "macos") {
        10
    } else {
        12
    }
}

/// Give both themes of `ctx` the colours, the spacing and the text sizes of
/// the window. The one that is in use follows the system, as before.
pub fn apply_style(ctx: &egui::Context) {
    for (theme, dark_mode) in [(Theme::Dark, true), (Theme::Light, false)] {
        ctx.set_visuals_of(theme, Tokens::for_mode(dark_mode).visuals(dark_mode));
    }
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(7.0, 3.0);
        style.spacing.interact_size.y = 22.0;
        style.spacing.menu_margin = egui::Margin::same(6);
        for (text_style, size) in [
            (TextStyle::Body, 13.0),
            (TextStyle::Button, 13.0),
            (TextStyle::Small, 11.0),
            (TextStyle::Heading, 15.0),
            (TextStyle::Monospace, 12.5),
        ] {
            let family = if text_style == TextStyle::Monospace {
                FontFamily::Monospace
            } else {
                FontFamily::Proportional
            };
            style
                .text_styles
                .insert(text_style, FontId::new(size, family));
        }
    });
}

/// The look of a style on a background that is dark or light.
#[must_use]
pub fn look(style: StyleKey, dark_mode: bool) -> Look {
    let foreground = if dark_mode {
        Color32::from_gray(220)
    } else {
        Color32::from_gray(20)
    };
    let dim = if dark_mode {
        Color32::from_gray(120)
    } else {
        Color32::from_gray(140)
    };
    let (color, width) = match style {
        StyleKey::Wire { conductors } => (
            live_color(conductors, foreground, dim),
            if conductors.is_empty() { 1.0 } else { 2.0 },
        ),
        StyleKey::Junction { conductors } => (live_color(conductors, foreground, dim), 1.0),
        StyleKey::Symbol | StyleKey::Label => (foreground, 1.5),
        StyleKey::Linkage => (dim, 1.0),
        StyleKey::Frame => (dim, 1.0),
        StyleKey::Energized | StyleKey::Actuated => (ENERGIZED, 2.0),
        StyleKey::Active | StyleKey::ContactClosed => (ACTIVE, 2.0),
        StyleKey::Tripped => (FAULT, 2.0),
        StyleKey::Motor(MotorState::Stopped) => (foreground, 1.5),
        StyleKey::Motor(MotorState::Forward) => (ACTIVE, 2.5),
        StyleKey::Motor(MotorState::Reverse) => (REVERSE, 2.5),
        StyleKey::Fault => (FAULT, 3.0),
        StyleKey::Selected => (SELECTED, 2.5),
        StyleKey::Traced => (TRACED, 2.5),
        StyleKey::Unknown => (CONFLICT, 1.5),
    };
    Look { color, width }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_wire_is_dim_and_thin() {
        let dead = look(
            StyleKey::Wire {
                conductors: ConductorSet::default(),
            },
            true,
        );

        assert_eq!(
            dead,
            Look {
                color: Color32::from_gray(120),
                width: 1.0
            }
        );
    }

    #[test]
    fn wire_with_one_conductor_takes_its_colour() {
        let live = look(
            StyleKey::Wire {
                conductors: ConductorSet::of(Conductor::N),
            },
            false,
        );

        assert_eq!(live.color, Color32::from_rgb(40, 110, 230));
    }

    #[test]
    fn wire_with_several_conductors_is_drawn_as_a_conflict() {
        let mut both = ConductorSet::of(Conductor::DcPos);
        both.insert(Conductor::DcNeg);

        assert_eq!(
            look(StyleKey::Wire { conductors: both }, true).color,
            CONFLICT
        );
    }

    #[test]
    fn black_phase_stays_visible_on_a_dark_background() {
        let l2 = look(
            StyleKey::Wire {
                conductors: ConductorSet::of(Conductor::L2),
            },
            true,
        );

        assert_eq!(l2.color, Color32::from_gray(220));
    }

    fn luminance(color: Color32) -> f32 {
        let [r, g, b, _] = color.to_array();
        0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)
    }

    #[test]
    fn the_dark_theme_is_dark_and_the_light_theme_is_light() {
        for tokens in [Tokens::DARK, Tokens::LIGHT] {
            let dark = tokens == Tokens::DARK;

            assert_eq!(luminance(tokens.canvas) < 128.0, dark);
            assert_eq!(luminance(tokens.chrome) < 128.0, dark);
            assert_eq!(luminance(tokens.text) > 128.0, dark);
        }
        assert_eq!(Tokens::for_mode(true), Tokens::DARK);
        assert_eq!(Tokens::for_mode(false), Tokens::LIGHT);
    }

    #[test]
    fn text_is_readable_on_the_chrome_in_both_themes() {
        for tokens in [Tokens::DARK, Tokens::LIGHT] {
            for (text, background) in [
                (tokens.text, tokens.chrome),
                (tokens.text, tokens.panel),
                (tokens.text, tokens.control),
                (tokens.text_dim, tokens.chrome),
            ] {
                let difference = (luminance(text) - luminance(background)).abs();

                assert!(
                    difference > 70.0,
                    "{text:?} on {background:?}: {difference}"
                );
            }
        }
    }

    /// WCAG relative luminance of an opaque colour.
    fn wcag_luminance(color: Color32) -> f64 {
        let linear = |channel: u8| {
            let value = f64::from(channel) / 255.0;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
    }

    /// WCAG contrast ratio between two opaque colours.
    fn contrast(a: Color32, b: Color32) -> f64 {
        let (a, b) = (wcag_luminance(a), wcag_luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// The text of the window reads at 4.5:1 or better (WCAG AA) on the
    /// surfaces it is drawn on, in both themes.
    #[test]
    fn text_meets_the_aa_contrast_on_the_surfaces_it_is_drawn_on() {
        for (name, tokens) in [("dark", Tokens::DARK), ("light", Tokens::LIGHT)] {
            let mut pairs = Vec::new();
            for surface in [
                tokens.chrome,
                tokens.chrome_dark,
                tokens.panel,
                tokens.canvas,
                tokens.command,
                tokens.control,
                tokens.control_hover,
                tokens.tab_active,
            ] {
                pairs.push(("text", tokens.text, surface));
            }
            for surface in [
                tokens.chrome,
                tokens.chrome_dark,
                tokens.panel,
                tokens.canvas,
                tokens.command,
            ] {
                pairs.push(("text_dim", tokens.text_dim, surface));
            }
            // The warning of the palette is drawn on the side panel, which
            // has the colour of the chrome, and the notes of the drawing
            // are on the canvas.
            pairs.push(("warn", tokens.warn, tokens.chrome));
            pairs.push(("warn", tokens.warn, tokens.panel));
            pairs.push(("text_faint", tokens.text_faint, tokens.canvas));
            // The main button of a dialog, and the danger colour over the
            // surface of a dialog.
            pairs.push(("on_accent", tokens.on_accent, tokens.accent_fill));
            pairs.push(("danger", tokens.danger, tokens.panel));
            pairs.push(("danger", tokens.danger, tokens.chrome));
            // Selected text: the selection fill is the accent at 45 %.
            for surface in [tokens.chrome, tokens.panel] {
                let blend = |accent: u8, base: u8| {
                    (f32::from(accent) * 0.45 + f32::from(base) * 0.55).round() as u8
                };
                let fill = Color32::from_rgb(
                    blend(tokens.accent.r(), surface.r()),
                    blend(tokens.accent.g(), surface.g()),
                    blend(tokens.accent.b(), surface.b()),
                );
                pairs.push(("text on selection", tokens.text, fill));
            }
            for (what, foreground, background) in pairs {
                let ratio = contrast(foreground, background);
                assert!(
                    ratio >= 4.5,
                    "{name}: {what} {foreground:?} on {background:?} is {ratio:.2}:1"
                );
            }
        }
    }

    #[test]
    fn applying_the_style_gives_each_theme_its_own_colours() {
        let ctx = egui::Context::default();

        apply_style(&ctx);

        let dark = ctx.style_of(Theme::Dark).visuals.clone();
        let light = ctx.style_of(Theme::Light).visuals.clone();
        assert_eq!(dark.panel_fill, Tokens::DARK.chrome);
        assert_eq!(light.panel_fill, Tokens::LIGHT.chrome);
        assert_eq!(dark.widgets.active.bg_fill, Tokens::DARK.accent);
        assert_eq!(
            ctx.style_of(Theme::Dark)
                .text_styles
                .get(&TextStyle::Body)
                .map(|font| font.size),
            Some(13.0)
        );
    }
}
