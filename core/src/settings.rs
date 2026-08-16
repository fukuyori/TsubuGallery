//! 設定 (設計書 §24)。
//!
//! 設定キーは言語に依存しない ASCII 文字列で、そのまま SQLite の `setting`
//! テーブルへ入れる。値も文字列にしておくと、項目を足したときに移行が要らない。
//! 読めない値や知らない値は既定値へ倒し、設定が壊れていても起動できるようにする。

use std::fmt;
use std::str::FromStr;

use crate::locale::LanguagePreference;

/// 選択肢がいくつかしかない設定を、文字列と往復させるための決まりごと。
pub trait Choice: Sized + Copy + PartialEq + 'static {
    /// 画面に並べる順の全選択肢。
    const ALL: &'static [Self];

    /// 保存に使う言語非依存のキー。
    fn key(self) -> &'static str;

    /// 表示名を引くための翻訳キー。
    fn locale_key(self) -> String;

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.key() == s)
    }
}

macro_rules! choice {
    ($(#[$meta:meta])* $name:ident, $prefix:literal, { $($variant:ident => $key:literal),+ $(,)? }, default = $default:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant),+ }

        impl Choice for $name {
            const ALL: &'static [Self] = &[$(Self::$variant),+];

            fn key(self) -> &'static str {
                match self { $(Self::$variant => $key),+ }
            }

            fn locale_key(self) -> String {
                format!(concat!($prefix, ".{}"), self.key())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::$default
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.key())
            }
        }

        impl FromStr for $name {
            type Err = ();
            fn from_str(s: &str) -> Result<Self, ()> {
                Self::parse(s).ok_or(())
            }
        }
    };
}

choice!(
    /// 配色。
    Theme, "settings.theme",
    { Dark => "dark", Light => "light" },
    default = Dark
);

choice!(
    /// 起動時に開く画面。
    StartScreen, "settings.start_screen",
    { Gallery => "gallery", Viewer => "viewer" },
    default = Gallery
);

choice!(
    /// 作品の並べ方 (設計書 §6.2)。
    ViewMode, "settings.view_mode",
    { Grid => "grid", LargeCard => "large_card", List => "list" },
    default = Grid
);

choice!(
    /// カードの大きさ。実際の列数は画面幅との兼ね合いで決まる。
    CardSize, "settings.card_size",
    { Small => "small", Medium => "medium", Large => "large" },
    default = Medium
);

choice!(
    /// 「次の作品」の選び方。
    Navigation, "settings.navigation",
    { Sequential => "sequential", Random => "random" },
    default = Sequential
);

choice!(
    /// `size()` で宣言されたキャンバスを、画面へどう当てはめるか。
    ///
    /// つぶやき系は正方形が多く、横長の画面では左右が余る。
    CanvasFit, "settings.canvas_fit",
    { Contain => "contain", Cover => "cover" },
    default = Contain
);

choice!(
    /// サムネイルの解像度。
    ImageQuality, "settings.image_quality",
    { Low => "low", Standard => "standard", High => "high" },
    default = Standard
);

choice!(
    /// 1 フレームに使ってよい命令数。重い作品を諦める閾値でもある。
    ExecutionBudget, "settings.execution_budget",
    { Low => "low", Standard => "standard", High => "high" },
    default = Standard
);

choice!(
    /// 目標フレームレート。
    FrameRate, "settings.frame_rate",
    { Fps30 => "30", Fps60 => "60" },
    default = Fps60
);

choice!(
    /// 作品の時計を何倍で進めるか。
    ///
    /// フレームレートとは別物。フレームレートは「1 秒に何回描くか」で、こちらは
    /// 「作品にとっての 1 秒が実時間の何秒か」。速い作品をゆっくり眺めたり、
    /// 動きの遅い作品を早送りしたりするためのもの。
    PlaybackSpeed, "settings.playback_speed",
    { Quarter => "0.25", Half => "0.5", Normal => "1", Double => "2", Quadruple => "4" },
    default = Normal
);

choice!(
    /// 無操作から自動再生を始めるまでの時間 (設計書 §27 のスクリーンセーバーモード)。
    ScreenSaver, "settings.screensaver",
    { Off => "off", After1 => "1", After3 => "3", After5 => "5", After10 => "10" },
    default = Off
);

choice!(
    /// 並び順。[`crate`] からは触らないが、設定として持ち回る。
    SortOrder, "gallery.sort",
    { Name => "name", RecentlyAdded => "recently_added", RecentlyOpened => "recently_opened" },
    default = Name
);

impl ViewMode {
    /// カードの大きさ設定が効くか。リストは 1 行 1 作品なので効かない。
    pub fn uses_card_size(self) -> bool {
        matches!(self, ViewMode::Grid | ViewMode::LargeCard)
    }
}

impl CardSize {
    /// カード幅の下限にかける倍率。
    pub fn scale(self) -> f32 {
        match self {
            CardSize::Small => 0.72,
            CardSize::Medium => 1.0,
            CardSize::Large => 1.4,
        }
    }
}

impl ScreenSaver {
    /// 待ち時間。`Off` なら `None`。
    pub fn idle(self) -> Option<std::time::Duration> {
        let minutes = match self {
            ScreenSaver::Off => return None,
            ScreenSaver::After1 => 1,
            ScreenSaver::After3 => 3,
            ScreenSaver::After5 => 5,
            ScreenSaver::After10 => 10,
        };
        Some(std::time::Duration::from_secs(minutes * 60))
    }
}

impl ImageQuality {
    /// サムネイルの横幅 (px)。
    pub fn width(self) -> u32 {
        match self {
            ImageQuality::Low => 320,
            ImageQuality::Standard => 640,
            ImageQuality::High => 1280,
        }
    }
}

impl ExecutionBudget {
    /// 1 フレームあたりの命令数の上限。
    pub fn instructions(self) -> u64 {
        match self {
            ExecutionBudget::Low => 5_000_000,
            ExecutionBudget::Standard => 20_000_000,
            ExecutionBudget::High => 80_000_000,
        }
    }
}

impl FrameRate {
    pub fn fps(self) -> f32 {
        match self {
            FrameRate::Fps30 => 30.0,
            FrameRate::Fps60 => 60.0,
        }
    }

    /// 1 フレームの長さ。
    pub fn interval(self) -> std::time::Duration {
        std::time::Duration::from_secs_f32(1.0 / self.fps())
    }
}

impl PlaybackSpeed {
    /// 作品の時計にかける倍率。
    pub fn multiplier(self) -> f32 {
        match self {
            PlaybackSpeed::Quarter => 0.25,
            PlaybackSpeed::Half => 0.5,
            PlaybackSpeed::Normal => 1.0,
            PlaybackSpeed::Double => 2.0,
            PlaybackSpeed::Quadruple => 4.0,
        }
    }

    /// 1 段速いほう。いちばん速ければそのまま。
    pub fn faster(self) -> Self {
        Self::step(self, 1)
    }

    /// 1 段遅いほう。いちばん遅ければそのまま。
    pub fn slower(self) -> Self {
        Self::step(self, -1)
    }

    /// 端で止める。巡回させると、4× の次に 0.25× が来て見失う。
    fn step(self, by: isize) -> Self {
        let at = Self::ALL.iter().position(|v| *v == self).unwrap_or(0) as isize;
        let next = (at + by).clamp(0, Self::ALL.len() as isize - 1) as usize;
        Self::ALL[next]
    }
}

/// サムネイルを撮るフレーム番号の範囲。
pub const CAPTURE_FRAME_RANGE: std::ops::RangeInclusive<u64> = 1..=600;

/// スライドショーの送り間隔 (秒)。
pub const SLIDESHOW_INTERVAL_RANGE: std::ops::RangeInclusive<u32> = 2..=120;

/// 設定一式。
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub language: LanguagePreference,
    pub theme: Theme,
    pub start_screen: StartScreen,

    pub view_mode: ViewMode,
    pub card_size: CardSize,
    pub sort_order: SortOrder,
    pub show_titles: bool,

    pub fullscreen: bool,
    /// キャンバスを画面へ収めるか、埋めるか。
    pub canvas_fit: CanvasFit,
    pub frame_rate: FrameRate,
    /// 作品の時計にかける倍率。フレームレートとは別に効く。
    pub playback_speed: PlaybackSpeed,
    pub navigation: Navigation,
    pub preload: bool,
    /// スライドショーで次へ送るまでの秒数 (設計書 §27)。
    pub slideshow_interval: u32,
    /// 無操作でスライドショーを始めるまでの時間。
    pub screensaver: ScreenSaver,

    pub capture_frame: u64,
    pub image_quality: ImageQuality,

    pub execution_budget: ExecutionBudget,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: LanguagePreference::System,
            theme: Theme::default(),
            start_screen: StartScreen::default(),
            view_mode: ViewMode::default(),
            card_size: CardSize::default(),
            sort_order: SortOrder::default(),
            show_titles: true,
            fullscreen: false,
            canvas_fit: CanvasFit::default(),
            frame_rate: FrameRate::default(),
            playback_speed: PlaybackSpeed::default(),
            navigation: Navigation::default(),
            preload: true,
            slideshow_interval: 10,
            screensaver: ScreenSaver::default(),
            capture_frame: 90,
            image_quality: ImageQuality::default(),
            execution_budget: ExecutionBudget::default(),
        }
    }
}

/// 言語設定を表す特別な値。`LanguagePreference::System` を保存するときに使う。
const LANGUAGE_SYSTEM: &str = "system";

impl Settings {
    /// キーと値の並びへ落とす。保存側はこれをそのまま書けばよい。
    pub fn to_pairs(&self) -> Vec<(&'static str, String)> {
        vec![
            (
                "language",
                match &self.language {
                    LanguagePreference::System => LANGUAGE_SYSTEM.to_string(),
                    LanguagePreference::Explicit(tag) => tag.clone(),
                },
            ),
            ("theme", self.theme.key().into()),
            ("start_screen", self.start_screen.key().into()),
            ("view_mode", self.view_mode.key().into()),
            ("card_size", self.card_size.key().into()),
            ("sort_order", self.sort_order.key().into()),
            ("show_titles", bool_key(self.show_titles).into()),
            ("fullscreen", bool_key(self.fullscreen).into()),
            ("canvas_fit", self.canvas_fit.key().into()),
            ("frame_rate", self.frame_rate.key().into()),
            ("playback_speed", self.playback_speed.key().into()),
            ("navigation", self.navigation.key().into()),
            ("preload", bool_key(self.preload).into()),
            ("slideshow_interval", self.slideshow_interval.to_string()),
            ("screensaver", self.screensaver.key().into()),
            ("capture_frame", self.capture_frame.to_string()),
            ("image_quality", self.image_quality.key().into()),
            ("execution_budget", self.execution_budget.key().into()),
        ]
    }

    /// キーと値の並びから組み立てる。読めない値は既定値のままにする。
    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut s = Self::default();
        for (key, value) in pairs {
            match key {
                "language" => {
                    s.language = if value == LANGUAGE_SYSTEM || value.is_empty() {
                        LanguagePreference::System
                    } else {
                        LanguagePreference::Explicit(value.to_string())
                    };
                }
                "theme" => set(&mut s.theme, value),
                "start_screen" => set(&mut s.start_screen, value),
                "view_mode" => set(&mut s.view_mode, value),
                "card_size" => set(&mut s.card_size, value),
                "sort_order" => set(&mut s.sort_order, value),
                "show_titles" => set_bool(&mut s.show_titles, value),
                "fullscreen" => set_bool(&mut s.fullscreen, value),
                "canvas_fit" => set(&mut s.canvas_fit, value),
                "frame_rate" => set(&mut s.frame_rate, value),
                "playback_speed" => set(&mut s.playback_speed, value),
                "navigation" => set(&mut s.navigation, value),
                "preload" => set_bool(&mut s.preload, value),
                "slideshow_interval" => {
                    if let Ok(n) = value.parse::<u32>() {
                        s.slideshow_interval = n.clamp(
                            *SLIDESHOW_INTERVAL_RANGE.start(),
                            *SLIDESHOW_INTERVAL_RANGE.end(),
                        );
                    }
                }
                "screensaver" => set(&mut s.screensaver, value),
                "capture_frame" => {
                    if let Ok(n) = value.parse::<u64>() {
                        s.capture_frame = n.clamp(*CAPTURE_FRAME_RANGE.start(), *CAPTURE_FRAME_RANGE.end());
                    }
                }
                "image_quality" => set(&mut s.image_quality, value),
                "execution_budget" => set(&mut s.execution_budget, value),
                other => log::debug!("知らない設定キーを読み飛ばします: {other}"),
            }
        }
        s
    }
}

fn set<T: Choice>(slot: &mut T, value: &str) {
    match T::parse(value) {
        Some(v) => *slot = v,
        None => log::warn!("設定の値を読めません: {value}"),
    }
}

fn set_bool(slot: &mut bool, value: &str) {
    match value {
        "on" | "true" | "1" => *slot = true,
        "off" | "false" | "0" => *slot = false,
        other => log::warn!("設定の値を読めません: {other}"),
    }
}

fn bool_key(v: bool) -> &'static str {
    if v { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_survive_a_round_trip() {
        let original = Settings::default();
        let pairs = original.to_pairs();
        let restored =
            Settings::from_pairs(pairs.iter().map(|(k, v)| (*k, v.as_str())));
        assert_eq!(original, restored);
    }

    #[test]
    fn every_field_survives_a_round_trip() {
        let original = Settings {
            language: LanguagePreference::Explicit("ja-JP".into()),
            theme: Theme::Light,
            start_screen: StartScreen::Viewer,
            view_mode: ViewMode::List,
            card_size: CardSize::Large,
            sort_order: SortOrder::RecentlyOpened,
            show_titles: false,
            fullscreen: true,
            canvas_fit: CanvasFit::Cover,
            frame_rate: FrameRate::Fps30,
            playback_speed: PlaybackSpeed::Double,
            navigation: Navigation::Random,
            preload: false,
            slideshow_interval: 45,
            screensaver: ScreenSaver::After3,
            capture_frame: 123,
            image_quality: ImageQuality::High,
            execution_budget: ExecutionBudget::High,
        };
        let pairs = original.to_pairs();
        let restored =
            Settings::from_pairs(pairs.iter().map(|(k, v)| (*k, v.as_str())));
        assert_eq!(original, restored, "往復で落ちた項目があります");
    }

    #[test]
    fn a_broken_value_falls_back_to_the_default() {
        let s = Settings::from_pairs([
            ("theme", "ちがう"),
            ("capture_frame", "abc"),
            ("card_size", "large"),
        ]);
        assert_eq!(s.theme, Theme::default());
        assert_eq!(s.capture_frame, Settings::default().capture_frame);
        assert_eq!(s.card_size, CardSize::Large, "読めた項目まで巻き添えにしてはいけない");
    }

    #[test]
    fn an_unknown_key_is_ignored() {
        let s = Settings::from_pairs([("no_such_setting", "1"), ("show_titles", "off")]);
        assert!(!s.show_titles);
    }

    #[test]
    fn the_screensaver_is_off_unless_asked_for() {
        assert_eq!(Settings::default().screensaver, ScreenSaver::Off);
        assert_eq!(ScreenSaver::Off.idle(), None, "切っているのに動き出してはいけない");
        assert_eq!(
            ScreenSaver::After3.idle(),
            Some(std::time::Duration::from_secs(180))
        );
    }

    #[test]
    fn the_slideshow_interval_is_kept_in_range() {
        assert_eq!(Settings::from_pairs([("slideshow_interval", "0")]).slideshow_interval, 2);
        assert_eq!(Settings::from_pairs([("slideshow_interval", "9999")]).slideshow_interval, 120);
    }

    #[test]
    fn capture_frame_is_kept_in_range() {
        assert_eq!(Settings::from_pairs([("capture_frame", "0")]).capture_frame, 1);
        assert_eq!(Settings::from_pairs([("capture_frame", "99999")]).capture_frame, 600);
    }

    #[test]
    fn system_language_is_not_stored_as_a_tag() {
        let s = Settings::default();
        let pairs = s.to_pairs();
        let language = pairs.iter().find(|(k, _)| *k == "language").expect("language");
        assert_eq!(language.1, "system");
    }

    #[test]
    fn choice_keys_are_unique_and_ascii() {
        fn check<T: Choice>() {
            let mut seen = Vec::new();
            for v in T::ALL {
                let key = v.key();
                assert!(key.is_ascii(), "設定キーは ASCII にする: {key}");
                assert!(!seen.contains(&key), "設定キーが重複しています: {key}");
                seen.push(key);
            }
        }
        check::<Theme>();
        check::<ViewMode>();
        check::<StartScreen>();
        check::<CardSize>();
        check::<Navigation>();
        check::<ScreenSaver>();
        check::<ImageQuality>();
        check::<ExecutionBudget>();
        check::<FrameRate>();
        check::<PlaybackSpeed>();
        check::<SortOrder>();
    }

    /// 倍率は表示している数字のとおり。
    #[test]
    fn the_speed_is_the_number_on_the_label() {
        for speed in <PlaybackSpeed as Choice>::ALL {
            let shown: f32 = speed.key().parse().expect("数字のキー");
            assert_eq!(speed.multiplier(), shown);
        }
    }

    /// 端で止まる。巡回すると 4× の次に 0.25× が来て、押しすぎに気付けない。
    #[test]
    fn stepping_the_speed_stops_at_the_ends() {
        let mut speed = PlaybackSpeed::Normal;
        for _ in 0..5 {
            speed = speed.faster();
        }
        assert_eq!(speed, PlaybackSpeed::Quadruple);
        for _ in 0..9 {
            speed = speed.slower();
        }
        assert_eq!(speed, PlaybackSpeed::Quarter);
        assert_eq!(speed.slower(), PlaybackSpeed::Quarter);
    }
}
