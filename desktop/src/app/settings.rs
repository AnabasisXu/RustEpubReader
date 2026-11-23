//! Settings persistence and the desktop snapshot adapter, kept out of the frame loop.
use super::{
    default_anim_speed, default_line_spacing, default_para_spacing, default_sans,
    default_scripts_enabled, default_text_indent, default_title_font_scale, AppView, ReaderApp,
};
use eframe::egui::Color32;
use reader_core::i18n::Language;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct AppSettings {
    #[serde(default)]
    pub(super) library_list_mode: bool,
    #[serde(default)]
    pub(super) library_sort_title: bool,
    #[serde(default = "default_scripts_enabled")]
    pub(super) scripts_enabled: bool,
    #[serde(default)]
    pub(super) mouse_page_turn: bool,
    #[serde(default = "super::default_shortcuts")]
    pub(super) shortcuts: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub(super) css_policy: reader_core::epub::native::CssPolicy,
    pub(super) font_size: f32,
    #[serde(default, deserialize_with = "optional_appearance_setting")]
    pub(super) reader_introduction_seen: Option<bool>,
    pub(super) dark_mode: bool,
    #[serde(default, deserialize_with = "optional_appearance_setting")]
    pub(super) appearance_linkage: Option<reader_core::appearance::AppearanceLinkage>,
    #[serde(default, deserialize_with = "optional_appearance_setting")]
    pub(super) appearance_paints: Option<crate::ui::appearance_linkage::ReadingPaintPair>,
    #[serde(default, deserialize_with = "optional_appearance_setting")]
    pub(super) reading_themes: Option<crate::ui::reading_themes::ReadingThemes>,
    #[serde(default, deserialize_with = "optional_appearance_setting")]
    pub(super) appearance_current_dark: Option<bool>,
    pub(super) reader_bg_color: [u8; 4],
    pub(super) reader_font_color: Option<[u8; 4]>,
    #[serde(default)]
    pub(super) reader_day_palette: Option<([u8; 4], Option<[u8; 4]>)>,
    pub(super) reader_font_family: String,
    #[serde(default = "default_sans")]
    pub(super) reader_cjk_font_family: String,
    #[serde(default)]
    pub(super) reader_title_font_family: String,
    pub(super) reader_page_animation: String,
    #[serde(default = "default_anim_speed")]
    pub(super) reader_page_animation_speed: f32,
    pub(super) reader_bg_image_path: Option<String>,
    pub(super) reader_bg_image_alpha: f32,
    pub(super) scroll_mode: bool,
    #[serde(default)]
    pub(super) force_single_column: bool,
    #[serde(default)]
    pub(super) fullscreen: bool,
    pub(super) show_toc: bool,
    #[serde(default)]
    pub(super) language: String,
    #[serde(default)]
    pub(super) last_book_path: Option<String>,
    #[serde(default)]
    pub(super) last_chapter: usize,
    #[serde(default = "default_line_spacing")]
    pub(super) line_spacing: f32,
    #[serde(default = "default_para_spacing")]
    pub(super) para_spacing: f32,
    #[serde(default = "default_text_indent")]
    pub(super) text_indent: u8,
    #[serde(default = "default_title_font_scale")]
    pub(super) title_font_scale: f32,
    #[serde(default)]
    pub(super) auto_scroll_speed: f32,
}

// A malformed new appearance field must not discard unrelated legacy settings.
fn optional_appearance_setting<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

impl AppSettings {
    pub(super) fn path(data_dir: &str) -> PathBuf {
        PathBuf::from(data_dir).join("settings.json")
    }

    pub(super) fn load(data_dir: &str) -> Option<Self> {
        let path = Self::path(data_dir);
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub(super) fn save(&self, data_dir: &str) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        reader_core::storage::replace(&Self::path(data_dir), &bytes)
    }

    fn from_color(c: Color32) -> [u8; 4] {
        c.to_array()
    }

    fn to_color(c: [u8; 4]) -> Color32 {
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
    }

    pub(super) fn from_app(app: &ReaderApp) -> Self {
        let (last_book_path, last_chapter) =
            if let Some((path, chapter, _)) = &app.pending_book_open {
                (
                    Some(path.clone()),
                    app.library
                        .books
                        .iter()
                        .find(|entry| &entry.path == path)
                        .map(|entry| entry.last_chapter)
                        .or(*chapter)
                        .unwrap_or(0),
                )
            } else if matches!(app.view, AppView::Reader) && !app.book_presentation.is_closing() {
                (app.book_path.clone(), app.current_chapter)
            } else {
                (None, 0)
            };

        let mut appearance_paints = app.appearance_paints.clone();
        if app.appearance_current_dark {
            appearance_paints.dark = app.capture_reading_paint();
        } else {
            appearance_paints.light = app.capture_reading_paint();
        }
        let mut reading_themes = app.reading_themes.clone();
        reading_themes.capture(appearance_paints.clone());
        Self {
            library_list_mode: app.library_list_mode,
            library_sort_title: app.library_sort_title,
            scripts_enabled: app.scripts_enabled,
            mouse_page_turn: app.mouse_page_turn,
            shortcuts: app.shortcuts.clone(),
            css_policy: app.css_policy,
            font_size: app.font_size,
            reader_introduction_seen: Some(app.reader_introduction_seen),
            dark_mode: app.dark_mode,
            appearance_linkage: Some(app.appearance_linkage),
            appearance_paints: Some(appearance_paints),
            reading_themes: Some(reading_themes),
            appearance_current_dark: Some(app.appearance_current_dark),
            reader_bg_color: Self::from_color(app.reader_bg_color),
            reader_day_palette: app
                .reader_day_palette
                .map(|(bg, ink)| (Self::from_color(bg), ink.map(Self::from_color))),
            reader_font_color: app.reader_font_color.map(Self::from_color),
            reader_font_family: app.reader_font_family.clone(),
            reader_cjk_font_family: app.reader_cjk_font_family.clone(),
            reader_title_font_family: app.reader_title_font_family.clone(),
            reader_page_animation: app.reader_page_animation.clone(),
            reader_page_animation_speed: app.reader_page_animation_speed,
            reader_bg_image_path: app.reader_bg_image_path.clone(),
            reader_bg_image_alpha: app.reader_bg_image_alpha,
            scroll_mode: app.scroll_mode,
            force_single_column: app.force_single_column,
            fullscreen: app.fullscreen,
            show_toc: app.show_toc,
            language: app.i18n.preference().code().to_string(),
            last_book_path,
            last_chapter,
            line_spacing: app.line_spacing,
            para_spacing: app.para_spacing,
            text_indent: app.text_indent,
            title_font_scale: app.title_font_scale,
            auto_scroll_speed: app.auto_scroll_speed,
        }
    }

    pub(super) fn apply_to_app(&self, app: &mut ReaderApp) {
        app.library_list_mode = self.library_list_mode;
        app.library_sort_title = self.library_sort_title;
        app.font_size = self.font_size.clamp(12.0, 40.0);
        app.reader_introduction_seen = self
            .reader_introduction_seen
            .unwrap_or(self.last_book_path.is_some());
        app.dark_mode = self.dark_mode;
        app.reader_bg_color = Self::to_color(self.reader_bg_color);
        app.reader_day_palette = self
            .reader_day_palette
            .map(|(bg, ink)| (Self::to_color(bg), ink.map(Self::to_color)));
        app.reader_font_color = self.reader_font_color.map(Self::to_color);
        app.reader_font_family = self.reader_font_family.clone();
        app.reader_cjk_font_family = self.reader_cjk_font_family.clone();
        app.reader_title_font_family = self.reader_title_font_family.clone();
        app.reader_page_animation = self.reader_page_animation.clone();
        app.reader_page_animation_speed = self.reader_page_animation_speed.clamp(0.04, 0.40);
        app.reader_bg_image_path = self.reader_bg_image_path.clone();
        app.reader_bg_image_alpha = self.reader_bg_image_alpha.clamp(0.0, 1.0);
        app.migrate_appearance(
            self.appearance_linkage,
            self.appearance_paints.clone(),
            self.appearance_current_dark,
        );
        app.reading_themes = self
            .reading_themes
            .clone()
            .map(|themes| themes.normalized())
            .unwrap_or_else(|| {
                crate::ui::reading_themes::ReadingThemes::migrate(app.appearance_paints.clone())
            });
        app.apply_selected_reading_theme();
        app.mouse_page_turn = self.mouse_page_turn;
        if !self.shortcuts.is_empty() {
            app.shortcuts = self.shortcuts.clone();
        }
        app.scroll_mode = self.scroll_mode;
        app.force_single_column = self.force_single_column;
        app.fullscreen = self.fullscreen;
        app.show_toc = self.show_toc;
        app.line_spacing = self.line_spacing.clamp(0.8, 2.5);
        app.para_spacing = self.para_spacing.clamp(0.0, 2.0);
        app.text_indent = self.text_indent.min(4);
        app.title_font_scale = self.title_font_scale.clamp(1.0, 2.5);
        app.auto_scroll_speed = self.auto_scroll_speed.clamp(0.0, 200.0);
        app.i18n.set_language(Language::from_code(&self.language));
        // last_book_path/last_chapter applied in Default::default after call
    }
}

