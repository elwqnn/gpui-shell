use gpui::{Pixels, px};
use serde::{Deserialize, Serialize};
use ui::Radius;

use super::modules::{
    ActiveWindowConfig, BatteryConfig, ClockConfig, ControlCenterWidgetConfig,
    KeyboardLayoutConfig, LauncherBtnConfig, MprisConfig, SysInfoConfig, TrayConfig,
    WorkspacesConfig,
};

/// Bar screen position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarPosition {
    /// Left edge of the screen.
    Left,
    /// Right edge of the screen.
    Right,
    /// Top edge of the screen.
    #[default]
    Top,
    /// Bottom edge of the screen.
    Bottom,
}

impl BarPosition {
    pub const ALL: [Self; 4] = [Self::Left, Self::Right, Self::Top, Self::Bottom];

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
        }
    }

    #[inline(always)]
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    /// The same edge, as the UI layer names it. Config keeps its own enum
    /// because it carries the serde spelling.
    pub fn edge(self) -> ui::patterns::BarEdge {
        use ui::patterns::BarEdge;
        match self {
            Self::Left => BarEdge::Left,
            Self::Right => BarEdge::Right,
            Self::Top => BarEdge::Top,
            Self::Bottom => BarEdge::Bottom,
        }
    }
}

/// How the bar meets the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarStyle {
    /// Flush against the screen edge.
    Attached,
    /// Inset from the screen edges with rounded corners (see `bar.floating`);
    /// the stock look.
    #[default]
    Floating,
    /// The old spelling of an attached bar with `[frame]` enabled, its
    /// geometry under `bar.frame`. Still read; not offered.
    Frame,
    /// A transparent bar where each entry of `start`, `center` and `end` is
    /// its own pill. A nested list (`["Clock", "Notifications"]`) shares one.
    Bubbles,
}

impl BarStyle {
    pub const ALL: [Self; 3] = [Self::Attached, Self::Floating, Self::Bubbles];

    pub fn label(self) -> &'static str {
        match self {
            Self::Attached => "Attached",
            Self::Floating => "Floating",
            Self::Frame => "Frame",
            Self::Bubbles => "Bubbles",
        }
    }
}

/// Geometry of a floating bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarFloatingConfig {
    /// Distance in px between the bar and the screen edges.
    pub gap: f32,
    /// Corner radius in px. Unset, it follows the theme's `radius_scale`
    /// (12px at scale 1); set, it is used as is.
    pub radius: Option<f32>,
}

impl Default for BarFloatingConfig {
    fn default() -> Self {
        Self {
            gap: 8.0,
            radius: None,
        }
    }
}

impl BarFloatingConfig {
    /// The token a floating bar's corners follow when `radius` is unset.
    pub const RADIUS: Radius = Radius::XLarge;

    pub fn radius(&self) -> Pixels {
        explicit_or_token(self.radius, Self::RADIUS)
    }
}

/// Geometry of the screen frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarFrameConfig {
    /// Width in px of the three frame edges the bar is not on. Windows are
    /// kept inside them.
    pub thickness: f32,
    /// Radius in px of the frame's inner corners. Unset, it follows the
    /// theme's `radius_scale` (16px at scale 1); set, it is used as is.
    pub radius: Option<f32>,
}

impl Default for BarFrameConfig {
    fn default() -> Self {
        Self {
            thickness: 6.0,
            radius: None,
        }
    }
}

impl BarFrameConfig {
    /// The token the frame's inner corners follow when `radius` is unset.
    pub const RADIUS: Radius = Radius::XXLarge;

    pub fn radius(&self) -> Pixels {
        explicit_or_token(self.radius, Self::RADIUS)
    }
}

/// An explicit radius wins over the theme-scaled token.
fn explicit_or_token(explicit: Option<f32>, token: Radius) -> Pixels {
    explicit.map_or_else(|| token.pixels(), |radius| px(radius.max(0.)))
}

/// Geometry of the segments widgets sit in: one per entry of `start`,
/// `center` and `end`. Each knob is optional and defaults from `bar.style`:
/// `bubbles` gets `gap = 6`, `inset = 3`, `padding = 4`; every other style
/// gets `0`, so segments are invisible unless you ask for room.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BarSegmentsConfig {
    /// Space in px between neighbouring segments.
    pub gap: Option<f32>,
    /// Space in px between a segment and the bar's long edges.
    pub inset: Option<f32>,
    /// Space in px inside a segment's two ends.
    pub padding: Option<f32>,
}

/// The resolved `[bar.segments]` knobs, in px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentSpacing {
    pub gap: f32,
    pub inset: f32,
    pub padding: f32,
}

impl BarSegmentsConfig {
    /// What `bubbles` gets when nothing is set.
    pub const BUBBLES: SegmentSpacing = SegmentSpacing {
        gap: 6.0,
        inset: 3.0,
        padding: 4.0,
    };
    pub const FLUSH: SegmentSpacing = SegmentSpacing {
        gap: 0.0,
        inset: 0.0,
        padding: 0.0,
    };

    pub fn resolve(&self, style: BarStyle) -> SegmentSpacing {
        let defaults = match style {
            BarStyle::Bubbles => Self::BUBBLES,
            BarStyle::Attached | BarStyle::Floating | BarStyle::Frame => Self::FLUSH,
        };
        SegmentSpacing {
            gap: self.gap.unwrap_or(defaults.gap).max(0.),
            inset: self.inset.unwrap_or(defaults.inset).max(0.),
            padding: self.padding.unwrap_or(defaults.padding).max(0.),
        }
    }
}

/// A section entry: one widget, or several sharing a bubble.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BarItem {
    Widget(String),
    Group(Vec<String>),
}

impl BarItem {
    pub fn names(&self) -> &[String] {
        match self {
            Self::Widget(name) => std::slice::from_ref(name),
            Self::Group(names) => names,
        }
    }
}

impl From<&str> for BarItem {
    fn from(name: &str) -> Self {
        Self::Widget(name.to_string())
    }
}

/// Status bar configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarConfig {
    /// Main axis thickness in px (height for horizontal, width for vertical).
    pub size: f32,
    /// Padding in px at the bar's two ends, before the first and after the
    /// last widget. Unset, the first chip sits as far from the end as chips
    /// sit from each other (with `bubbles`, the segment gap; with
    /// `widget_stretch`, nothing).
    pub padding: Option<f32>,
    /// Screen edge where the bar is placed.
    pub position: BarPosition,
    /// `attached`, `floating`, `frame` or `bubbles`.
    pub style: BarStyle,
    /// Opacity of the bar (and frame) background, from 0.0 (transparent) to
    /// 1.0. Unset, it follows `opacity` in `theme.toml`. `background` is
    /// accepted as an older spelling.
    #[serde(alias = "background")]
    pub opacity: Option<f32>,
    /// Used when `style = "floating"`.
    pub floating: BarFloatingConfig,
    /// Used when `style = "frame"`.
    pub frame: BarFrameConfig,
    /// Space between and around the segments that hold the widgets.
    pub segments: BarSegmentsConfig,
    /// Whether the bar draws a border: the screen-facing edge when attached,
    /// all around when floating, the inner outline when framed, around each
    /// pill with bubbles.
    pub show_border: bool,
    /// Whether widgets render a resting background fill.
    pub widget_background: bool,
    /// Whether widgets render a subtle border.
    pub widget_border: bool,
    /// Whether widget chips and workspace cells fill their segment's full
    /// thickness with no margin, so they sit flush and square against each
    /// other (the look of a blocky bar). Corners still follow the theme's
    /// `radius_scale`.
    pub widget_stretch: bool,
    /// Start section widgets (left for horizontal, top for vertical).
    pub start: Vec<BarItem>,
    /// Center section widgets.
    pub center: Vec<BarItem>,
    /// End section widgets (right for horizontal, bottom for vertical).
    pub end: Vec<BarItem>,
    /// Bar modules
    pub modules: ModulesConfig,
}

/// Bar module configurations.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModulesConfig {
    pub clock: ClockConfig,
    pub battery: BatteryConfig,
    pub workspaces: WorkspacesConfig,
    pub tray: TrayConfig,
    pub sysinfo: SysInfoConfig,
    pub mpris: MprisConfig,
    pub active_window: ActiveWindowConfig,
    pub keyboard_layout: KeyboardLayoutConfig,
    pub launcher_btn: LauncherBtnConfig,
    pub control_center: ControlCenterWidgetConfig,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            size: 32.0,
            padding: None,
            position: BarPosition::default(),
            style: BarStyle::default(),
            opacity: None,
            floating: BarFloatingConfig::default(),
            frame: BarFrameConfig::default(),
            segments: BarSegmentsConfig::default(),
            show_border: true,
            widget_background: true,
            widget_border: true,
            widget_stretch: false,
            start: vec!["LauncherBtn".into(), "Workspaces".into(), "SysInfo".into()],
            center: vec!["ActiveWindow".into()],
            end: vec![
                "Clock".into(),
                "Mpris".into(),
                "Notifications".into(),
                "Systray".into(),
                "KeyboardLayout".into(),
                "ControlCenter".into(),
            ],
            modules: ModulesConfig::default(),
        }
    }
}

impl BarConfig {
    #[inline(always)]
    pub fn is_vertical(&self) -> bool {
        self.position.is_vertical()
    }

    /// Distance from the screen edge to the bar's far side: what the bar
    /// takes away from the rest of the output.
    pub fn depth(&self) -> f32 {
        match self.style {
            BarStyle::Floating => self.floating.gap + self.size,
            BarStyle::Attached | BarStyle::Frame | BarStyle::Bubbles => self.size,
        }
    }

    /// The bar background at its opacity, or the theme's.
    pub fn background_color(&self, theme: &ui::Theme) -> gpui::Hsla {
        theme.surface(self.opacity)
    }

    /// The resolved `[bar.segments]` knobs for this style. A stretched bar
    /// has no chip chrome to tell entries apart, so its segments keep a
    /// breath of padding at their ends unless the config says otherwise.
    pub fn segment_spacing(&self) -> SegmentSpacing {
        let mut spacing = self.segments.resolve(self.style);
        if self.widget_stretch && self.segments.padding.is_none() {
            spacing.padding = f32::from(self.rhythm().breath);
        }
        spacing
    }

    /// Px the bar's own hairline takes off its thickness: one edge when
    /// attached, both when floating; the frame draws its own and bubbles
    /// move the border onto the segments.
    fn border_thickness(&self) -> f32 {
        if !self.show_border {
            return 0.;
        }
        match self.style {
            BarStyle::Attached => 1.,
            BarStyle::Floating => 2.,
            BarStyle::Frame | BarStyle::Bubbles => 0.,
        }
    }

    /// What a segment gets across the bar: the size minus the bar's border,
    /// its edge padding and the segment inset on both sides. Explicit, so a
    /// stretched chip or a square workspace cell is exact rather than
    /// "fill whatever is left".
    pub fn segment_thickness(&self) -> f32 {
        (self.content_thickness() - self.segment_spacing().inset * 2.).max(0.)
    }

    /// What the bar has across it once its own hairline and edge padding
    /// are taken off; what the rhythm derives from, inset or not, so a
    /// bubbles bar keeps the same icons and chips as an attached one.
    pub fn content_thickness(&self) -> f32 {
        let edge = if self.is_vertical() {
            f32::from(ui::patterns::BarSurface::VERTICAL_EDGE_PADDING) * 2.
        } else {
            0.
        };
        (self.size - self.border_thickness() - edge).max(0.)
    }

    /// The spacing grid everything in and around the bar is laid out on.
    pub fn rhythm(&self) -> ui::patterns::BarRhythm {
        ui::patterns::BarRhythm::derive(px(self.content_thickness()))
    }

    /// How far a panel opened from the bar sits from it and from the screen
    /// edges: a floating bar's gap, otherwise one breath of the bar's rhythm.
    pub fn panel_gap(&self) -> f32 {
        match self.style {
            BarStyle::Floating => self.floating.gap.max(0.),
            BarStyle::Attached | BarStyle::Frame | BarStyle::Bubbles => {
                f32::from(self.rhythm().breath)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A section mixes plain names with nested lists, the bubbles that hold several widgets.
    #[test]
    fn sections_take_widgets_and_groups() {
        let raw = concat!(
            "style = \"bubbles\"\n",
            "start = [\"LauncherBtn\", [\"Clock\", \"Notifications\"]]\n",
        );

        let bar: BarConfig = toml::from_str(raw).expect("bar config parses");

        assert_eq!(bar.style, BarStyle::Bubbles);
        assert_eq!(bar.start[0], BarItem::from("LauncherBtn"));
        assert_eq!(
            bar.start[1].names(),
            ["Clock".to_string(), "Notifications".to_string()]
        );
    }

    #[test]
    fn a_floating_bar_takes_its_gap_from_the_output() {
        let bar = BarConfig {
            style: BarStyle::Floating,
            ..BarConfig::default()
        };
        assert_eq!(bar.depth(), bar.size + bar.floating.gap);
    }

    /// `background` was the opacity's name before the theme got one; a
    /// config that still uses it keeps its bar.
    #[test]
    fn background_is_an_alias_for_opacity() {
        let bar: BarConfig = toml::from_str("background = 0.8\n").expect("bar config parses");
        assert_eq!(bar.opacity, Some(0.8));
        let bar: BarConfig = toml::from_str("opacity = 0.5\n").expect("bar config parses");
        assert_eq!(bar.opacity, Some(0.5));
        assert_eq!(BarConfig::default().opacity, None);
    }

    /// An explicit radius is taken literally; an unset one follows the token.
    #[test]
    fn radii_default_to_tokens_and_explicit_values_win() {
        let floating = BarFloatingConfig::default();
        assert_eq!(floating.radius(), BarFloatingConfig::RADIUS.pixels());
        let floating = BarFloatingConfig {
            radius: Some(3.0),
            ..Default::default()
        };
        assert_eq!(floating.radius(), px(3.0));

        let frame = BarFrameConfig::default();
        assert_eq!(frame.radius(), BarFrameConfig::RADIUS.pixels());
        let frame = BarFrameConfig {
            radius: Some(-4.0),
            ..Default::default()
        };
        assert_eq!(frame.radius(), px(0.0));
    }

    /// Segment knobs default per style and an explicit one wins.
    #[test]
    fn segment_spacing_defaults_per_style() {
        let bar = BarConfig::default();
        assert_eq!(bar.segment_spacing(), BarSegmentsConfig::FLUSH);

        let bar = BarConfig {
            style: BarStyle::Bubbles,
            ..Default::default()
        };
        assert_eq!(bar.segment_spacing(), BarSegmentsConfig::BUBBLES);

        let bar: BarConfig = toml::from_str("style = \"bubbles\"\n[segments]\ngap = 12\n")
            .expect("bar config parses");
        let spacing = bar.segment_spacing();
        assert_eq!(spacing.gap, 12.0);
        assert_eq!(spacing.inset, BarSegmentsConfig::BUBBLES.inset);
        assert_eq!(spacing.padding, BarSegmentsConfig::BUBBLES.padding);
        assert!(!bar.widget_stretch);
    }

    /// The thickness handed to a segment is what the bar has left after its
    /// own hairline and the inset, so a stretched chip never overflows.
    #[test]
    fn segment_thickness_subtracts_border_and_inset() {
        let attached = BarConfig {
            style: BarStyle::Attached,
            position: BarPosition::Top,
            size: 32.0,
            ..Default::default()
        };
        assert_eq!(attached.segment_thickness(), 31.0);

        let borderless = BarConfig {
            show_border: false,
            ..attached.clone()
        };
        assert_eq!(borderless.segment_thickness(), 32.0);

        let floating = BarConfig {
            style: BarStyle::Floating,
            ..attached.clone()
        };
        assert_eq!(floating.segment_thickness(), 30.0);

        let bubbles = BarConfig {
            style: BarStyle::Bubbles,
            ..attached.clone()
        };
        assert_eq!(bubbles.segment_thickness(), 32.0 - 2.0 * 3.0);

        let vertical = BarConfig {
            position: BarPosition::Left,
            ..attached
        };
        assert_eq!(vertical.segment_thickness(), 32.0 - 1.0 - 2.0);
    }

    /// The two reference bars from the spec parse as written.
    #[test]
    fn reference_configs_parse() {
        let blocky: BarConfig = toml::from_str(concat!(
            "position = \"top\"\n",
            "style = \"attached\"\n",
            "show_border = false\n",
            "widget_background = false\n",
            "widget_border = false\n",
            "widget_stretch = true\n",
            "padding = 0\n",
            "start = [\"LauncherBtn\", \"Workspaces\", \"SysInfo\"]\n",
            "center = [\"ActiveWindow\"]\n",
            "end = [\"Mpris\", \"Notifications\", [\"Systray\", \"KeyboardLayout\"], \"Settings\", \"Clock\"]\n",
        ))
        .expect("blocky config parses");
        assert!(blocky.widget_stretch);
        assert_eq!(blocky.padding, Some(0.0));
        assert_eq!(blocky.end.len(), 5);
        assert_eq!(blocky.segment_thickness(), blocky.size);
        // Flush segments still pad their ends by a breath, so entries read
        // as blocks without colour fills.
        assert_eq!(
            blocky.segment_spacing().padding,
            f32::from(blocky.rhythm().breath)
        );
        assert_eq!(blocky.segment_spacing().gap, 0.0);

        let rounded: BarConfig = toml::from_str(concat!(
            "position = \"top\"\n",
            "style = \"bubbles\"\n",
            "widget_background = false\n",
            "widget_border = false\n",
            "start = [\"LauncherBtn\", \"Workspaces\", \"SysInfo\"]\n",
            "center = [\"ActiveWindow\"]\n",
            "end = [[\"Systray\", \"KeyboardLayout\"], \"Settings\", [\"Clock\", \"Notifications\"]]\n",
        ))
        .expect("rounded config parses");
        assert_eq!(rounded.segment_spacing(), BarSegmentsConfig::BUBBLES);
        assert_eq!(rounded.end[2].names().len(), 2);
    }

    /// `Frame` is read for old files but no longer offered: the frame is
    /// its own section.
    #[test]
    fn position_and_style_catalogues_are_complete() {
        assert_eq!(BarPosition::ALL.len(), 4);
        assert_eq!(BarStyle::ALL.len(), 3);
        assert!(!BarStyle::ALL.contains(&BarStyle::Frame));
        assert!(BarPosition::ALL.iter().all(|p| !p.label().is_empty()));
        assert!(BarStyle::ALL.iter().all(|s| !s.label().is_empty()));
        let legacy: BarConfig = toml::from_str("style = \"frame\"").expect("still parses");
        assert_eq!(legacy.style, BarStyle::Frame);
    }
}
