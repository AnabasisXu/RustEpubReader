//! The Desktop application state machine, defining main app logic.
/// 可配置快捷键：action -> 组合键写法，如 "Ctrl+B"、"F2"、"Alt+1"。
pub(crate) fn default_shortcuts() -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    for (name, spec) in [
        ("annotations", "Ctrl+B"),
        ("library", "Ctrl+W"),
        ("open", "Ctrl+O"),
        ("search", "Ctrl+F"),
        ("toc", "Ctrl+T"),
        ("settings", "Ctrl+,"),
        ("highlight", "Ctrl+A"),
        ("notes_search", "Ctrl+Shift+F"),
        ("notes_page", "F9"),
        ("tag_focus", "Alt+T"),
        ("bookmark", "Ctrl+M"),
        ("quick_open", "F10"),
        ("vim_mode", "Alt+V"),
        ("undo_highlight", "Ctrl+Z"),
        ("export_notes", "Alt+E"),
        ("font_smaller", "Ctrl+-"),
        ("font_larger", "Ctrl+="),
    ] {
        map.insert(name.to_string(), spec.to_string());
    }
    map
}

mod opening;
mod keymap;
mod settings;
mod shortcuts;
use settings::AppSettings;

use std::collections::HashMap;
use std::fmt::Write as _;
#[cfg(not(target_os = "macos"))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::Color32;

use reader_core::epub::EpubBook;
use reader_core::i18n::{I18n, Language};
use reader_core::library::Library;

use crate::ui::reader_state::ContinuousScrollState;

type FontDiscoveryResult = Arc<
    Mutex<
        Option<(
            Vec<String>,
            HashMap<String, crate::system_fonts::SystemFont>,
            std::collections::HashSet<String>,
        )>,
    >,
>;

type PendingBookOpen = (
    String,
    Option<usize>,
    std::sync::mpsc::Receiver<Result<(EpubBook, Option<String>), String>>,
);

fn extend_unique_font_chain(target: &mut Vec<String>, fonts: impl IntoIterator<Item = String>) {
    for font in fonts {
        if !target.contains(&font) {
            target.push(font);
        }
    }
}

fn default_data_dir() -> String {
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir.to_string_lossy().to_string()
}

#[cfg(not(target_os = "macos"))]
fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "ttf" | "otf" | "ttc" | "otc"
            )
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn collect_font_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 5 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_font_files(&path, depth + 1, out);
        } else if is_font_file(&path) {
            out.push(path);
        }
    }
}

pub(crate) fn is_cjk_font_name(s: &str) -> bool {
    s.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
        || s.contains("CJK")
        || s.ends_with(" SC")
        || s.ends_with(" TC")
        || ["PingFang", "Songti", "Heiti", "Kaiti", "Hiragino Sans GB"]
            .iter()
            .any(|name| s.contains(name))
}

fn default_sans() -> String {
    if cfg!(target_os = "macos") {
        "苹方-简"
    } else {
        "Sans"
    }
    .to_string()
}

#[cfg(not(target_os = "macos"))]
fn discover_system_fonts() -> (
    Vec<String>,
    HashMap<String, crate::system_fonts::SystemFont>,
    std::collections::HashSet<String>,
) {
    // 已知字体文件 stem → 人性化显示名（优先匹配，避免显示 "msyh" 之类的文件名）
    let known_names: HashMap<&str, &str> = [
        // Windows 中文字体
        ("msyh", "微软雅黑"),
        ("msyhbd", "微软雅黑 粗体"),
        ("msyhl", "微软雅黑 细体"),
        ("simsun", "宋体 (SimSun)"),
        ("simsunb", "宋体-ExtB"),
        ("simhei", "黑体 (SimHei)"),
        ("simkai", "楷体 (SimKai)"),
        ("simfang", "仿宋 (FangSong)"),
        ("simli", "隶书 (SimLi)"),
        ("simyou", "幼圆 (YouYuan)"),
        ("STXIHEI", "华文细黑"),
        ("STKAITI", "华文楷体"),
        ("STFANGSO", "华文仿宋"),
        ("STSONG", "华文宋体"),
        ("STLITI", "华文隶书"),
        ("STXINWEI", "华文新魏"),
        ("STZHONGS", "华文中宋"),
        ("STHUPO", "华文琥珀"),
        ("STCAIYUN", "华文彩云"),
        ("FZSTK", "方正舒体"),
        ("FZYTK", "方正姚体"),
        ("FZLTH", "方正兰亭黑"),
        ("FZLTZHK", "方正兰亭中黑"),
        ("mingliu", "細明體 (MingLiU)"),
        ("kaiu", "標楷體 (KaiU)"),
        // macOS 中文字体
        ("PingFang SC", "苹方-简"),
        ("PingFang TC", "苹方-繁"),
        ("PingFang", "苹方"),
        ("Songti SC", "宋体-简"),
        ("Songti TC", "宋体-繁"),
        ("Heiti SC", "黑体-简"),
        ("Heiti TC", "黑体-繁"),
        ("Kaiti SC", "楷体-简"),
        ("Kaiti TC", "楷体-繁"),
        ("STSong", "华文宋体"),
        ("STKaiti", "华文楷体"),
        ("STFangsong", "华文仿宋"),
        ("STHeiti", "华文黑体"),
        // Noto CJK（跨平台）
        ("NotoSansCJK-Regular", "Noto Sans CJK"),
        ("NotoSansCJKsc-Regular", "Noto Sans CJK SC"),
        ("NotoSansCJKtc-Regular", "Noto Sans CJK TC"),
        ("NotoSerifCJK-Regular", "Noto Serif CJK"),
        ("NotoSerifCJKsc-Regular", "Noto Serif CJK SC"),
        ("NotoSansSC-Regular", "Noto Sans SC"),
        ("NotoSerifSC-Regular", "Noto Serif SC"),
        // 思源字体
        ("SourceHanSans-Regular", "思源黑体"),
        ("SourceHanSerif-Regular", "思源宋体"),
        ("SourceHanSansCN-Regular", "思源黑体 CN"),
        ("SourceHanSerifCN-Regular", "思源宋体 CN"),
    ]
    .iter()
    .cloned()
    .collect();

    let mut roots: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "windows")]
    {
        roots.push(PathBuf::from("C:\\Windows\\Fonts"));
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            roots.push(PathBuf::from(local).join("Microsoft\\Windows\\Fonts"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        roots.push(PathBuf::from("/usr/share/fonts"));
        roots.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = std::env::var("HOME") {
            roots.push(PathBuf::from(home).join(".fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/System/Library/Fonts"));
        roots.push(PathBuf::from("/Library/Fonts"));
        if let Ok(home) = std::env::var("HOME") {
            roots.push(PathBuf::from(home).join("Library/Fonts"));
        }
    }

    let mut files = Vec::new();
    for root in roots {
        if root.exists() {
            collect_font_files(&root, 0, &mut files);
        }
    }

    let mut font_map = HashMap::new();
    let mut chosen_style: HashMap<String, String> = HashMap::new();
    let mut cjk = std::collections::HashSet::new();
    for path in files {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let friendly = known_names.get(stem).copied();
        let path_string = path.to_string_lossy().to_string();
        let faces = crate::system_fonts::font_faces(&path_string);
        if faces.is_empty() {
            let cleaned = stem.replace(['_', '-'], " ").trim().to_string();
            if cleaned.is_empty() {
                continue;
            }
            let is_cjk = is_cjk_font_name(&cleaned);
            font_map
                .entry(cleaned.clone())
                .or_insert_with(|| crate::system_fonts::SystemFont {
                    path: path_string.clone(),
                    index: 0,
                });
            if is_cjk {
                cjk.insert(cleaned);
            }
            continue;
        }
        let face_count = faces.len();
        for face in faces {
            let display = if face_count > 1 {
                format!("{} #{}", face.family, face.index + 1)
            } else {
                face.family.clone()
            };
            let mut aliases = vec![display];
            if let Some(localized) = face.localized {
                aliases.push(localized);
            }
            if let Some(friendly) = friendly {
                aliases.push(friendly.to_string());
            }
            for name in aliases {
                let name = name.trim().to_string();
                if name.is_empty() {
                    continue;
                }
                let regular = |style: &str| {
                    style.is_empty()
                        || style.eq_ignore_ascii_case("Regular")
                        || style.eq_ignore_ascii_case("Roman")
                };
                let replace = chosen_style
                    .get(&name)
                    .is_some_and(|existing| !regular(existing) && regular(&face.style));
                if chosen_style.get(&name).is_none() || replace {
                    font_map.insert(
                        name.clone(),
                        crate::system_fonts::SystemFont {
                            path: path_string.clone(),
                            index: face.index,
                        },
                    );
                    chosen_style.insert(name.clone(), face.style.clone());
                }
                if face.has_cjk {
                    cjk.insert(name);
                }
            }
        }
    }

    // CJK 字体排前面，其余按字母升序
    let mut names: Vec<String> = font_map.keys().cloned().collect();
    names.sort_by(|a, b| {
        let a_cjk = cjk.contains(a) || is_cjk_font_name(a);
        let b_cjk = cjk.contains(b) || is_cjk_font_name(b);
        match (a_cjk, b_cjk) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()),
        }
    });
    (names, font_map, cjk)
}

fn default_anim_speed() -> f32 {
    0.14
}

fn default_line_spacing() -> f32 {
    1.8
}

fn default_para_spacing() -> f32 {
    0.6
}

fn default_text_indent() -> u8 {
    2
}

#[cfg(target_os = "macos")]
fn discover_system_fonts() -> (
    Vec<String>,
    HashMap<String, crate::system_fonts::SystemFont>,
    std::collections::HashSet<String>,
) {
    let fonts = crate::system_fonts::discover();
    let mut names: Vec<_> = fonts.keys().cloned().collect();
    names.sort_by_key(|name| (!is_cjk_font_name(name), name.to_lowercase()));
    let cjk = names
        .iter()
        .filter(|name| is_cjk_font_name(name))
        .cloned()
        .collect();
    (names, fonts, cjk)
}

fn default_title_font_scale() -> f32 {
    1.5
}

#[derive(PartialEq)]
pub enum AppView {
    Library,
    Notes,
    Reader,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsTab {
    #[default]
    Appearance,
    Shortcuts,
}

/// Custom text selection state (replaces egui's native selectable label).
#[derive(Clone, Debug)]
pub struct TextSelection {
    /// Chapter-level block index where the drag started.
    pub start_block: usize,
    /// Char offset within start_block.
    pub start_char: usize,
    /// Chapter-level block index where the drag currently ends.
    pub end_block: usize,
    /// Char offset within end_block.
    pub end_char: usize,
    /// True while the user is still dragging.
    pub is_dragging: bool,
}

impl TextSelection {
    /// Returns (first_block, first_char) regardless of drag direction.
    pub fn normalized(&self) -> (usize, usize) {
        if self.start_block < self.end_block
            || (self.start_block == self.end_block && self.start_char <= self.end_char)
        {
            (self.start_block, self.start_char)
        } else {
            (self.end_block, self.end_char)
        }
    }

    /// Returns (start_block, start_char, end_block, end_char) in order: start <= end.
    pub fn normalized_range(&self) -> (usize, usize, usize, usize) {
        if self.start_block < self.end_block
            || (self.start_block == self.end_block && self.start_char <= self.end_char)
        {
            (
                self.start_block,
                self.start_char,
                self.end_block,
                self.end_char,
            )
        } else {
            (
                self.end_block,
                self.end_char,
                self.start_block,
                self.start_char,
            )
        }
    }
}

pub struct ReaderApp {
    pub pending_batch_import: Option<std::sync::mpsc::Receiver<Vec<String>>>,
    pub(crate) scripts_enabled: bool,
    pub(crate) native_events:
        std::collections::HashMap<usize, Vec<reader_core::epub::native::NativeEvent>>,
    pub(crate) native_applied:
        std::collections::HashMap<usize, (reader_core::epub::native::CssPolicy, bool, usize)>,
    pub(crate) native_book_path: Option<String>,
    pub(crate) publication_note: Option<reader_core::epub::native::NativeNote>,
    pub(crate) css_policy: reader_core::epub::native::CssPolicy,
    pub data_dir: String,
    pub book: Option<EpubBook>,
    pub book_path: Option<String>,
    pub current_book_hash: Option<String>,
    pub last_synced_chapter: Option<usize>,
    pub last_synced_block: Option<usize>,
    pub(crate) last_synced_char_offset: Option<usize>,
    pub current_chapter: usize,
    pub current_block: usize,
    pub(crate) current_char_offset: usize,
    pub pending_restore_block: Option<usize>,
    pub position_save_due: Option<std::time::Instant>,
    pub position_save_modified_ms: u64,
    pub font_size: f32,
    pub dark_mode: bool,
    pub appearance_linkage: reader_core::appearance::AppearanceLinkage,
    pub appearance_paints: crate::ui::appearance_linkage::ReadingPaintPair,
    pub reading_themes: crate::ui::reading_themes::ReadingThemes,
    pub reading_theme_editor: Option<crate::ui::reading_themes::ReadingThemeDraft>,
    pub appearance_current_dark: bool,
    appearance_transition: crate::ui::appearance_transition::AppearanceTransition,
    pub system_appearance_dark: bool,
    pub show_appearance_linkage: bool,
    pub appearance_selected_node: usize,
    pub reader_bg_color: Color32,
    pub reader_day_palette: Option<(Color32, Option<Color32>)>,
    pub reader_font_color: Option<Color32>,
    pub reader_font_family: String,
    pub reader_cjk_font_family: String,
    pub reader_title_font_family: String,
    pub title_font_search: String,
    pub reader_page_animation: String,
    pub reader_page_animation_speed: f32,
    pub reader_bg_image_path: Option<String>,
    pub reader_bg_image_alpha: f32,
    pub reader_bg_texture: Option<egui::TextureHandle>,
    pub show_settings: bool,
    pub settings_tab: SettingsTab,
    pub editing_note_buf: String,
    pub editing_note_original: Option<String>,
    pub(crate) shortcut_chord: Option<(char, std::time::Instant)>,
    pub(crate) reader_introduction_seen: bool,
    pub(crate) show_reader_introduction: bool,
    pub confirm_display_reset: bool,
    pub show_toc: bool,
    pub show_app_navigation: bool,
    pub scroll_to_top: bool,
    pub error_msg: Option<String>,
    pub view: AppView,
    pub pending_book_open: Option<PendingBookOpen>,
    pub(crate) pending_open_cancel: Option<reader_core::open_operation::Cancellation>,
    pub opening_started: Option<std::time::Instant>,
    pub(crate) book_presentation: crate::ui::book_presentation::BookPresentation,
    pub category_ui: crate::ui::categories::CategoryUi,
    pub library_manage: bool,
    pub library_selected: std::collections::HashSet<String>,
    pub library_confirm_delete: bool,
    pub library_show_archive: bool,
    pub library_archive_delete: Option<reader_core::cloud_sync::CloudBook>,
    pub library_query: String,
    pub library_sort_title: bool,
    pub library_list_mode: bool,
    pub library: Library,
    pub scroll_mode: bool,
    pub mouse_page_turn: bool,
    pub shortcuts: std::collections::BTreeMap<String, String>,
    pub shortcut_capture: Option<String>,
    pub show_notes_search: bool,
    pub notes_search_query: String,
    pub notes_search_results: Vec<crate::ui::notes_search::NoteHit>,
    pub notes_search_just_opened: bool,
    pub notes_page: crate::ui::notes_page::NotesPage,
    pub quick_open: crate::ui::quick_open::QuickOpen,
    pub bookmark_toast: Option<(String, f64)>,
    pub vim: crate::ui::vim::VimState,
    /// Vim 叠加层状态指纹；变化时丢弃卷页缓存，确保标签/光标立即重绘。
    pub vim_paint_key: u64,
    pub note_popup: Option<crate::ui::note_popup::NotePopup>,
    pub last_highlight_undo: Option<String>,
    pub(crate) continuous_scroll: ContinuousScrollState,
    pub current_page: usize,
    pub total_pages: usize,
    pub(crate) reader_render_blocks: Option<(usize, Arc<Vec<reader_core::epub::ContentBlock>>)>,
    pub page_block_ranges: crate::ui::reader_pagination::PageRanges,
    pub pages_dirty: bool,
    pub cover_textures: HashMap<String, Option<egui::TextureHandle>>,
    pub(crate) shelf_covers: crate::ui::shelf_covers::ShelfCovers,
    pub last_avail_width: f32,
    pub last_avail_height: f32,
    pub page_anim_from: usize,
    pub page_anim_to: usize,
    pub page_anim_progress: f32,
    pub page_anim_direction: f32,
    pub page_anim_cross_chapter: bool,
    pub page_anim_cross_chapter_snapshot: Option<CrossChapterSnapshot>,
    pub(crate) reader_layout_anchor: Option<(usize, usize)>,
    pub(crate) panel_reflow_anchor: Option<(usize, usize)>,
    pub(crate) page_curl: crate::ui::curl_reader::PageCurlState,
    pub(crate) reader_chrome: crate::ui::reader_chrome::ReaderChromeState,
    pub(crate) reader_navigation: crate::ui::reader_navigation::ReaderNavigation,
    pub is_dual_column: bool,
    pub force_single_column: bool,
    pub paging_page_rect: Option<egui::Rect>,
    pub embedded_font_names: Vec<String>,
    pub embedded_fonts_registered: bool,
    pub defer_custom_font_for_frame: bool,
    /// Font family the user originally chose; kept so we can retry after async
    /// font discovery completes (the fallback may have temporarily reset
    /// `reader_font_family` to "Sans").
    pending_font_family: Option<String>,
    pending_cjk_font_family: Option<String>,
    pub system_font_names: Vec<String>,
    pub(crate) system_font_paths: HashMap<String, crate::system_fonts::SystemFont>,
    pub(crate) cjk_font_names: std::collections::HashSet<String>,
    font_discovery_result: FontDiscoveryResult,
    last_saved_settings: Option<AppSettings>,
    pub font_search: String,
    pub cjk_font_search: String,
    pub i18n: I18n,
    language_checked: std::time::Instant,
    pub scroll_toc_to_current: bool,
    // ── Feedback ──
    pub feedback_logs: Arc<Mutex<Vec<String>>>,
    pub last_exported_feedback_log: Option<String>,
    pub show_about: bool,
    pub show_language: bool,
    pub about_icon_texture: Option<egui::TextureHandle>,
    // ── Self-update ──
    pub update_state: UpdateState,
    pub show_update_dialog: bool,
    pub update_latest_tag: Option<String>,
    pub _update_check_slot: Option<Arc<Mutex<Option<UpdateState>>>>,
    pub _update_download_slot: Option<Arc<Mutex<Option<UpdateState>>>>,
    pub _update_progress: Option<Arc<Mutex<f32>>>,
    // ── TXT Import ──
    pub txt_import: Option<TxtImportState>,
    // ── Typography ──
    pub line_spacing: f32,
    pub para_spacing: f32,
    pub text_indent: u8,
    pub title_font_scale: f32,
    // ── Search ──
    pub show_search: bool,
    pub search_query: String,
    #[cfg(feature = "performance-checks")]
    pub(crate) reader_performance: Option<crate::performance::ReaderPerformance>,
    #[cfg(feature = "performance-checks")]
    pub(crate) native_corpus: Option<crate::native_corpus::NativeCorpus>,
    #[cfg(feature = "performance-checks")]
    pub(crate) shelf_performance: Option<crate::shelf_performance::ShelfPerformance>,
    pub(crate) search_task: Option<crate::ui::search::SearchTask>,
    pub search_results: Vec<reader_core::search::SearchResult>,
    pub search_selected: Option<usize>,
    // ── Annotations ──
    pub show_annotations: bool,
    pub(crate) annotations_panel: crate::ui::annotations::PanelState,
    pub(crate) annotation_export: Option<crate::ui::annotations::AnnotationExportTask>,
    pub book_config: Option<reader_core::library::BookConfig>,
    // ── Auto-scroll ──
    #[allow(dead_code)]
    pub auto_scroll: bool,
    pub auto_scroll_speed: f32,
    // ── Custom text selection ──
    pub text_selection: Option<TextSelection>,
    pub sel_toolbar_pos: egui::Pos2,
    /// Pending drag origin: (press_pos, block_idx, char_idx). Created on press, promoted to
    /// TextSelection only when pointer moves > threshold. Cleared on release if no drag occurred.
    pub sel_press_origin: Option<(egui::Pos2, usize, usize)>,
    /// When set, the user clicked on a highlighted region → show note popup for this highlight.
    /// Note editing in annotations panel: highlight id being edited
    pub editing_note_id: Option<String>,
    pub last_egui_ctx: Option<egui::Context>,
    // ── Review Panel (段评覆盖层) ──
    pub show_review_panel: bool,
    pub review_panel_chapter: Option<usize>,
    pub review_panel_anchor: Option<String>,
    pub review_panel_just_opened: bool,
    /// Computed scroll offset for the current anchor. Applied once when opening.
    pub review_panel_scroll_offset: Option<f32>,
    /// When true, show all reviews; when false, filter to the anchored paragraph only.
    pub review_panel_show_all: bool,
}

#[derive(Debug, Clone, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    Available(String),
    Downloading,
    UpToDate,
    Failed(String),
    Restarting,
    DownloadPageOpened,
    CheckFailed(String),
}

impl UpdateState {
    pub(crate) fn from_update_check(result: anyhow::Result<Option<(String, String)>>) -> Self {
        match result {
            Ok(Some((tag, _))) => Self::Available(tag),
            Ok(None) => Self::UpToDate,
            Err(error) => Self::CheckFailed(format!("{error:#}")),
        }
    }
}

#[derive(Clone)]
pub struct CrossChapterSnapshot {
    pub chapter: usize,
    pub blocks: Arc<Vec<reader_core::epub::ContentBlock>>,
    pub block_ranges: crate::ui::reader_pagination::PageRanges,
    pub total_pages: usize,
    pub from_page: usize,
    pub title: String,
}

pub type TxtConvertSlot = Arc<Mutex<Option<Result<reader_core::txt::ConvertResult, String>>>>;

#[derive(Clone, Default)]
pub enum TxtImportPage {
    #[default]
    Review,
    Rules,
    Source,
    Excerpt(reader_core::txt::ChapterPreview),
}

/// TXT 导入对话框状态。
pub struct TxtImportState {
    pub page: TxtImportPage,
    pub txt_path: PathBuf,
    pub title: String,
    pub author: String,
    pub custom_regex: String,
    pub use_heuristic: bool,
    pub previews: Vec<reader_core::txt::ChapterPreview>,
    pub excluded: std::collections::BTreeSet<usize>,
    pub included: std::collections::BTreeSet<usize>,
    pub heading_count: usize,
    pub chapter_count: usize,
    pub source_query: String,
    pub source_hits: Vec<reader_core::txt::ChapterPreview>,
    pub pending: Option<(
        String,
        std::sync::mpsc::Receiver<Result<serde_json::Value, String>>,
    )>,
    pub analyzed: bool,
    pub converting: bool,
    pub error: Option<String>,
    /// 后台转换结果回传。
    pub result_slot: Option<TxtConvertSlot>,
}

impl TxtImportState {
    pub fn new(txt_path: PathBuf) -> Self {
        let title = txt_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let (previews, error) = (Vec::new(), None);

        Self {
            page: TxtImportPage::Review,
            txt_path,
            title,
            author: String::new(),
            custom_regex: String::new(),
            use_heuristic: true,
            previews,
            excluded: Default::default(),
            included: Default::default(),
            heading_count: 0,
            chapter_count: 0,
            source_query: String::new(),
            source_hits: Vec::new(),
            pending: None,
            analyzed: false,
            converting: false,
            error,
            result_slot: None,
        }
    }
}

impl Default for ReaderApp {
    fn default() -> Self {
        Self::from_data_dir(default_data_dir())
    }
}

impl ReaderApp {
    pub(crate) fn reload_library(&mut self) {
        match Library::load_from(&self.data_dir) {
            Ok(library) => self.library = library,
            Err(error) => self.error_msg = Some(error),
        }
    }

    pub(crate) fn from_data_dir(data_dir: String) -> Self {
        let (library, library_error) = match Library::load_from(&data_dir) {
            Ok(library) => (library, None),
            Err(error) => (Library::default(), Some(error)),
        };
        let font_discovery_result: FontDiscoveryResult = Arc::new(Mutex::new(None));
        {
            let result_slot = font_discovery_result.clone();
            std::thread::spawn(move || {
                let (names, paths, cjk) = discover_system_fonts();
                let fonts = (names, paths, cjk);
                if let Ok(mut slot) = result_slot.lock() {
                    *slot = Some(fonts);
                }
            });
        }
        let mut app = Self {
            pending_batch_import: None,
            data_dir,
            book: None,
            scripts_enabled: true,
            native_events: Default::default(),
            native_applied: Default::default(),
            native_book_path: None,
            publication_note: None,
            css_policy: Default::default(),
            book_path: None,
            current_book_hash: None,
            last_synced_chapter: None,
            last_synced_block: None,
            last_synced_char_offset: None,
            current_chapter: 0,
            current_block: 0,
            current_char_offset: 0,
            pending_restore_block: None,
            position_save_due: None,
            position_save_modified_ms: 0,
            font_size: 16.0,
            dark_mode: true,
            appearance_linkage: Default::default(),
            appearance_paints: Default::default(),
            reading_themes: Default::default(),
            reading_theme_editor: None,
            appearance_current_dark: false,
            appearance_transition: Default::default(),
            system_appearance_dark: false,
            show_appearance_linkage: false,
            appearance_selected_node: 0,
            reader_bg_color: Color32::from_rgb(250, 246, 238),
            reader_day_palette: None,
            reader_font_color: None,
            reader_font_family: "Sans".to_string(),
            reader_cjk_font_family: default_sans(),
            reader_title_font_family: String::new(),
            title_font_search: String::new(),
            reader_page_animation: "Realistic".to_string(),
            reader_page_animation_speed: 0.14,
            reader_bg_image_path: None,
            reader_bg_image_alpha: 0.22,
            reader_bg_texture: None,
            show_settings: false,
            settings_tab: SettingsTab::Appearance,
            editing_note_buf: String::new(),
            editing_note_original: None,
            shortcut_chord: None,
            reader_introduction_seen: false,
            show_reader_introduction: false,
            confirm_display_reset: false,
            show_toc: true,
            show_app_navigation: true,
            scroll_to_top: false,
            error_msg: library_error,
            view: AppView::Library,
            pending_book_open: None,
            pending_open_cancel: None,
            opening_started: None,
            book_presentation: Default::default(),
            category_ui: Default::default(),
            library_manage: false,
            library_selected: Default::default(),
            library_confirm_delete: false,
            library_show_archive: false,
            library_archive_delete: None,
            library_query: String::new(),
            library_sort_title: false,
            library_list_mode: false,
            library,
            scroll_mode: false,
            mouse_page_turn: false,
            shortcuts: default_shortcuts(),
            shortcut_capture: None,
            show_notes_search: false,
            notes_search_query: String::new(),
            notes_search_results: Vec::new(),
            notes_search_just_opened: false,
            notes_page: Default::default(),
            quick_open: Default::default(),
            bookmark_toast: None,
            vim: Default::default(),
            vim_paint_key: 0,
            note_popup: None,
            last_highlight_undo: None,
            continuous_scroll: ContinuousScrollState::default(),
            current_page: 0,
            total_pages: 0,
            reader_render_blocks: None,
            page_block_ranges: Default::default(),
            pages_dirty: true,
            cover_textures: HashMap::new(),
            shelf_covers: Default::default(),
            last_avail_width: 0.0,
            last_avail_height: 0.0,
            page_anim_from: 0,
            page_anim_to: 0,
            page_anim_progress: 1.0,
            page_anim_direction: 1.0,
            page_anim_cross_chapter: false,
            page_anim_cross_chapter_snapshot: None,
            reader_layout_anchor: None,
            panel_reflow_anchor: None,
            page_curl: Default::default(),
            reader_chrome: Default::default(),
            reader_navigation: Default::default(),
            is_dual_column: false,
            force_single_column: false,
            paging_page_rect: None,
            embedded_font_names: Vec::new(),
            embedded_fonts_registered: true,
            defer_custom_font_for_frame: false,
            pending_font_family: None,
            pending_cjk_font_family: None,
            system_font_names: Vec::new(),
            system_font_paths: HashMap::new(),
            cjk_font_names: std::collections::HashSet::new(),
            font_discovery_result,
            last_saved_settings: None,
            font_search: String::new(),
            cjk_font_search: String::new(),
            i18n: I18n::default(),
            language_checked: std::time::Instant::now(),
            scroll_toc_to_current: false,
            feedback_logs: Arc::new(Mutex::new(Vec::new())),
            last_exported_feedback_log: None,
            show_about: false,
            show_language: false,
            about_icon_texture: None,
            update_state: UpdateState::Idle,
            show_update_dialog: false,
            update_latest_tag: None,
            _update_check_slot: None,
            _update_download_slot: None,
            _update_progress: None,
            txt_import: None,
            // Typography
            line_spacing: default_line_spacing(),
            para_spacing: default_para_spacing(),
            text_indent: default_text_indent(),
            title_font_scale: default_title_font_scale(),
            // Search
            show_search: false,
            search_query: String::new(),
            #[cfg(feature = "performance-checks")]
            reader_performance: None,
            #[cfg(feature = "performance-checks")]
            native_corpus: None,
            #[cfg(feature = "performance-checks")]
            shelf_performance: None,
            search_task: None,
            search_results: Vec::new(),
            search_selected: None,
            // Annotations
            show_annotations: false,
            annotations_panel: Default::default(),
            annotation_export: None,
            book_config: None,
            // Auto-scroll
            auto_scroll: false,
            auto_scroll_speed: 30.0,
            text_selection: None,
            sel_toolbar_pos: egui::Pos2::ZERO,
            sel_press_origin: None,
            editing_note_id: None,
            last_egui_ctx: None,
            // Review Panel
            show_review_panel: false,
            review_panel_chapter: None,
            review_panel_anchor: None,
            review_panel_just_opened: false,
            review_panel_scroll_offset: None,
            review_panel_show_all: true,
        };

        if let Some(settings) = AppSettings::load(&app.data_dir) {
            app.scripts_enabled = settings.scripts_enabled;
            app.css_policy = settings.css_policy;
            app.push_feedback_log(format!("[Init] settings loaded: lang={}, font={}, font_size={}, scroll_mode={}, last_book={:?}, ch={}",
                settings.language, settings.reader_font_family, settings.font_size, settings.scroll_mode,
                settings.last_book_path.as_deref().unwrap_or("none"), settings.last_chapter));
            settings.apply_to_app(&mut app);
            app.embedded_fonts_registered = false;
            app.pages_dirty = true;
            // 自动恢复上次阅读的书籍
            if let Some(ref path) = settings.last_book_path.clone() {
                if std::path::Path::new(path).exists() {
                    app.push_feedback_log(format!("[Init] restoring last book: {}", path));
                    app.open_book_from_path(path, Some(settings.last_chapter));
                } else {
                    app.push_feedback_log(format!("[Init] last book not found: {}", path));
                }
            }
        } else {
            app.push_feedback_log("[Init] no saved settings found, using defaults");
        }
        app.push_feedback_log(format!(
            "[Init] app initialized (data_dir={})",
            app.data_dir
        ));
        app.last_saved_settings = Some(AppSettings::from_app(&app));

        // ── 启动时检查更新 ──
        #[cfg(not(test))]
        {
            let slot: Arc<Mutex<Option<UpdateState>>> = Arc::new(Mutex::new(None));
            let slot_clone = slot.clone();
            std::thread::spawn(move || {
                let result =
                    UpdateState::from_update_check(crate::self_update::check_latest_version());
                if let Ok(mut s) = slot_clone.lock() {
                    *s = Some(result);
                }
            });
            app._update_check_slot = Some(slot);
            app.update_state = UpdateState::Checking;
        }
        app
    }
}

impl ReaderApp {
    pub fn trigger_page_animation_to(&mut self, target_page: usize, direction: f32) {
        if self.reader_page_animation == "Realistic" && !self.scroll_mode {
            self.request_curl_turn(direction >= 0., Some(target_page));
            return;
        }
        if self.reader_page_animation == "None"
            || self.scroll_mode
            || target_page == self.current_page
        {
            self.current_page = target_page;
            self.page_anim_progress = 1.0;
            self.page_anim_from = target_page;
            self.page_anim_to = target_page;
            return;
        }

        self.page_anim_from = self.current_page;
        self.page_anim_to = target_page;
        self.page_anim_direction = if direction == 0.0 {
            1.0
        } else {
            direction.signum()
        };
        self.page_anim_progress = 0.0;
        self.page_anim_cross_chapter = false;
        self.current_page = target_page;
    }

    pub fn ensure_reader_bg_texture(&mut self, ctx: &egui::Context) {
        if self.reader_bg_texture.is_some() {
            return;
        }
        let Some(path) = &self.reader_bg_image_path else {
            return;
        };

        if let Ok(img) = image::open(path) {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            let tex = ctx.load_texture("reader_bg", color_image, egui::TextureOptions::LINEAR);
            self.reader_bg_texture = Some(tex);
        }
    }

    pub fn open_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("EPUB / TXT", &["epub", "txt"])
            .add_filter("EPUB", &["epub"])
            .add_filter("TXT", &["txt"])
            .pick_files()
        {
            if path.len() > 1 {
                self.import_epub_batch(path);
                return;
            }
            let Some(path) = path.into_iter().next() else {
                return;
            };
            let is_txt = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("txt"))
                .unwrap_or(false);

            if is_txt {
                self.txt_import = Some(TxtImportState::new(path));
            } else {
                let path_str = path.to_string_lossy().to_string();
                self.open_book_from_path(&path_str, None);
            }
        }
    }

    pub fn total_chapters(&self) -> usize {
        self.book.as_ref().map(|b| b.chapters.len()).unwrap_or(0)
    }

    pub fn next_chapter(&mut self) {
        let total = self.total_chapters();
        if total > 0 && self.current_chapter < total - 1 {
            self.current_chapter += 1;
            self.scroll_to_top = true;
            self.pages_dirty = true;
            self.current_page = 0;
            self.current_block = 0;
            self.current_char_offset = 0;
            self.pending_restore_block = None;
            self.position_save_modified_ms = reader_core::now_millis();
            self.position_save_due = Some(std::time::Instant::now());
            if let Err(error) = self.flush_reading_position() {
                self.error_msg = Some(error);
            }
        }
    }

    pub fn prev_chapter(&mut self) {
        if self.current_chapter > 0 {
            self.current_chapter -= 1;
            self.scroll_to_top = true;
            self.pages_dirty = true;
            self.current_page = 0;
            self.current_block = 0;
            self.current_char_offset = 0;
            self.pending_restore_block = None;
            self.position_save_modified_ms = reader_core::now_millis();
            self.position_save_due = Some(std::time::Instant::now());
            if let Err(error) = self.flush_reading_position() {
                self.error_msg = Some(error);
            }
        }
    }

    /// Save current book_config to disk.
    #[allow(dead_code)]
    pub fn save_book_config(&mut self) {
        if let Some(cfg) = &mut self.book_config {
            if let Err(error) = cfg.save(&self.data_dir) {
                self.error_msg = Some(error.to_string());
            };
        }
    }

    /// Capture a snapshot of the current chapter state for cross-chapter animation.
    pub fn capture_cross_chapter_snapshot(&mut self) {
        if self.scroll_mode || self.reader_page_animation == "None" {
            return;
        }
        if let Some(book) = &self.book {
            if let Some(ch) = book.chapters.get(self.current_chapter) {
                self.page_anim_cross_chapter_snapshot = Some(CrossChapterSnapshot {
                    chapter: self.current_chapter,
                    blocks: Arc::new(ch.blocks.clone()),
                    block_ranges: self.page_block_ranges.clone(),
                    total_pages: self.total_pages,
                    from_page: self.current_page,
                    title: ch.title.clone(),
                });
            }
        }
    }

    /// Begin a cross-chapter animation after chapter change.
    pub fn start_cross_chapter_animation(&mut self, direction: f32) {
        if self.scroll_mode || self.reader_page_animation == "None" {
            return;
        }
        self.page_anim_cross_chapter = true;
        self.page_anim_from = 0;
        self.page_anim_to = if direction < 0.0 { usize::MAX } else { 0 };
        self.page_anim_progress = 0.0;
        self.page_anim_direction = direction;
    }

    pub fn next_page(&mut self) {
        if self.reader_page_animation == "Realistic" && !self.scroll_mode {
            self.request_curl_turn(true, None);
            return;
        }
        if self.current_page + 1 < self.total_pages {
            self.trigger_page_animation_to(self.current_page + 1, 1.0);
        } else {
            self.capture_cross_chapter_snapshot();
            self.next_chapter();
            self.start_cross_chapter_animation(1.0);
        }
    }

    pub(crate) fn schedule_position_save_at(
        &mut self,
        chapter: usize,
        block: usize,
        char_offset: usize,
    ) {
        if self.current_chapter != chapter
            || self.current_block != block
            || self.current_char_offset != char_offset
        {
            self.current_chapter = chapter;
            self.current_block = block;
            self.current_char_offset = char_offset;
            self.position_save_modified_ms = reader_core::now_millis();
            self.position_save_due =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(400));
        }
    }

    pub(crate) fn flush_reading_position(&mut self) -> Result<(), String> {
        if let Some(due) = self.position_save_due.take() {
            if let Err(error) = self.persist_reading_position() {
                self.position_save_due = Some(due);
                return Err(error);
            }
        }
        Ok(())
    }
    fn flush_reading_position_if_due(&mut self) {
        let Some(due) = self.position_save_due else {
            return;
        };
        if std::time::Instant::now() < due {
            return;
        }
        self.position_save_due = None;
        if let Err(error) = self.persist_reading_position() {
            self.error_msg = Some(error);
        }
    }
    fn persist_reading_position(&mut self) -> Result<(), String> {
        let Some(path) = self.book_path.clone() else {
            return Ok(());
        };
        if self.position_save_modified_ms == 0 {
            self.position_save_modified_ms = reader_core::now_millis();
        }
        let title = self
            .book
            .as_ref()
            .and_then(|b| b.chapters.get(self.current_chapter))
            .map(|c| c.title.clone());
        self.library.apply_position_at(
            &self.data_dir,
            &path,
            self.current_chapter,
            title,
            self.current_block,
            self.current_char_offset,
            self.position_save_modified_ms,
        )?;
        self.last_synced_chapter = Some(self.current_chapter);
        self.last_synced_block = Some(self.current_block);
        self.last_synced_char_offset = Some(self.current_char_offset);
        Ok(())
    }

    pub fn prev_page(&mut self) {
        if self.reader_page_animation == "Realistic" && !self.scroll_mode {
            self.request_curl_turn(false, None);
            return;
        }
        if self.current_page > 0 {
            self.trigger_page_animation_to(self.current_page - 1, -1.0);
        } else if self.current_chapter > 0 {
            self.capture_cross_chapter_snapshot();
            self.prev_chapter();
            self.current_page = usize::MAX;
            self.start_cross_chapter_animation(-1.0);
        }
    }

    pub fn push_feedback_log(&self, msg: impl AsRef<str>) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let line = format!("[{ts}] {}", msg.as_ref());
        if reader_core::diagnostics::is_debug_logging_enabled() {
            eprintln!("[FEEDBACK-LOG] {line}");
        }

        let mut logs = self.feedback_logs.lock().unwrap_or_else(|e| e.into_inner());
        logs.push(line);
        if logs.len() > 600 {
            let remove = logs.len().saturating_sub(600);
            logs.drain(0..remove);
        }
    }

    pub fn export_feedback_log(&self) -> Result<String, String> {
        let log_dir = PathBuf::from(&self.data_dir).join("logs");
        std::fs::create_dir_all(&log_dir).map_err(|e| format!("create log dir: {e}"))?;

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let file_path = log_dir.join(format!("feedback_log_{ts}.txt"));

        let mut output = String::new();
        let _ = writeln!(output, "RustEpubReader Desktop Feedback Log");
        let _ = writeln!(output, "generated_unix_ts={ts}");
        let _ = writeln!(output, "app_version={}", env!("CARGO_PKG_VERSION"));
        let _ = writeln!(output, "data_dir={}", self.data_dir);
        let _ = writeln!(output, "books_count={}", self.library.books.len());
        let _ = writeln!(output, "language={}", self.i18n.language().code());
        let _ = writeln!(output, "---");
        let _ = writeln!(output, "Recent Books:");
        for b in self.library.books.iter().take(30) {
            let _ = writeln!(
                output,
                "- {} | {} | chapter={} | last_opened={}",
                b.title, b.path, b.last_chapter, b.last_opened
            );
        }
        let _ = writeln!(output, "---");
        let _ = writeln!(output, "Event Logs:");

        let logs = self
            .feedback_logs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for line in logs {
            let _ = writeln!(output, "{line}");
        }

        std::fs::write(&file_path, output).map_err(|e| format!("write log file: {e}"))?;
        Ok(file_path.to_string_lossy().to_string())
    }

    fn render_update_dialog(&mut self, ctx: &egui::Context) {
        let tag = self.update_latest_tag.clone().unwrap_or_default();
        let mut open = self.show_update_dialog;
        let mut close_requested = false;
        egui::Window::new(self.i18n.t("update.check"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(self.i18n.tf1("update.new_version", &tag))
                            .size(16.0)
                            .strong(),
                    );
                    ui.add_space(12.0);

                    match &self.update_state {
                        UpdateState::Available(_) => {
                            let button_size = egui::vec2(140.0, 32.0);
                            ui.horizontal_centered(|ui| {
                                if ui
                                    .add_sized(
                                        button_size,
                                        egui::Button::new(
                                            egui::RichText::new(
                                                self.i18n.t("update.download_update"),
                                            )
                                            .size(14.0),
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.update_state = UpdateState::Downloading;
                                    let ctx = ui.ctx().clone();
                                    let done_slot: Arc<Mutex<Option<UpdateState>>> =
                                        Arc::new(Mutex::new(None));
                                    let slot = done_slot.clone();
                                    let progress_for_ui: Arc<Mutex<f32>> =
                                        Arc::new(Mutex::new(0.0));
                                    let progress_writer = progress_for_ui.clone();
                                    std::thread::spawn(move || {
                                        let cb_ctx = ctx.clone();
                                        let pw = progress_writer;
                                        let result = crate::self_update::perform_update(Some(
                                            Box::new(move |downloaded, total| {
                                                if total > 0 {
                                                    let pct = downloaded as f32 / total as f32;
                                                    if let Ok(mut p) = pw.lock() {
                                                        *p = pct;
                                                    }
                                                    cb_ctx.request_repaint();
                                                }
                                            }),
                                        ));
                                        let state = match result {
                                            Ok(
                                                crate::self_update::UpdateOutcome::UpdateLaunched,
                                            ) => UpdateState::Restarting,
                                            Ok(crate::self_update::UpdateOutcome::DownloadPageOpened) => UpdateState::DownloadPageOpened,
                                            Ok(_) => UpdateState::UpToDate,
                                            Err(e) => UpdateState::Failed(e.to_string()),
                                        };
                                        if let Ok(mut s) = slot.lock() {
                                            *s = Some(state);
                                        }
                                        ctx.request_repaint();
                                    });
                                    self._update_download_slot = Some(done_slot);
                                    self._update_progress = Some(progress_for_ui);
                                }
                                if ui
                                    .add_sized(
                                        button_size,
                                        egui::Button::new(
                                            egui::RichText::new(self.i18n.t("feedback.not_now"))
                                                .size(14.0),
                                        ),
                                    )
                                    .clicked()
                                {
                                    close_requested = true;
                                }
                            });
                        }
                        UpdateState::Downloading => {
                            let pct = self
                                ._update_progress
                                .as_ref()
                                .and_then(|p| p.lock().ok().map(|v| *v))
                                .unwrap_or(0.0);
                            ui.label(
                                egui::RichText::new(self.i18n.t("update.downloading")).size(13.0),
                            );
                            ui.add(egui::ProgressBar::new(pct).show_percentage());
                            ui.ctx().request_repaint();
                        }
                        UpdateState::Failed(msg) => {
                            ui.label(
                                egui::RichText::new(self.i18n.tf1("update.failed", msg))
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(255, 100, 100)),
                            );
                            ui.add_space(4.0);
                            if ui.button(self.i18n.t("update.check")).clicked() {
                                close_requested = true;
                                self.update_state = UpdateState::Idle;
                            }
                        }
                        UpdateState::Restarting => {
                            ui.label(
                                egui::RichText::new(self.i18n.t("update.restarting"))
                                    .size(14.0)
                                    .color(egui::Color32::from_rgb(80, 200, 120)),
                            );
                        }
                        UpdateState::DownloadPageOpened => {
                            ui.label(self.i18n.t("update.download_page_opened"));
                        }
                        _ => {}
                    }
                    ui.add_space(8.0);
                });
            });
        if close_requested {
            open = false;
        }
        self.show_update_dialog = open;
    }
}

impl Drop for ReaderApp {
    fn drop(&mut self) {
        if let Some(cancel) = self.pending_open_cancel.take() {
            cancel.cancel();
        }
        if self.book_path.is_some() && self.position_save_due.is_some()
        {
            if let Err(error) = self.flush_reading_position() {
                eprintln!("Cannot flush reading position: {error}");
            }
        }
        if let Err(error) = AppSettings::from_app(self).save(&self.data_dir) {
            eprintln!("{error}");
        }
    }
}

impl eframe::App for ReaderApp {
    #[cfg(any(debug_assertions, feature = "performance-checks"))]
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        #[cfg(feature = "performance-checks")]
        if let Some(probe) = self.native_corpus.as_mut() {
            probe.raw_input(input)
        }
        #[cfg(debug_assertions)]
        if std::env::args().any(|arg| arg == "--verify-navigation")
            && _ctx
                .data(|data| data.get_temp::<(u32, f64)>(egui::Id::new("reader_navigation_check")))
                .is_some_and(|(step, _)| step == 3)
        {
            if let Some(paper) = self.paging_page_rect {
                input.focused = true;
                input.events.push(egui::Event::PointerMoved(egui::pos2(
                    paper.left() + 38.,
                    paper.bottom() - 38.,
                )));
            }
        }
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(feature = "performance-checks")]
        let performance_frame = std::time::Instant::now();
        #[cfg(feature = "performance-checks")]
        if let Some(probe) = self.native_corpus.as_mut() {
            probe.before_input(ctx);
        }
        self.sync_appearance(ctx);
        self.last_egui_ctx = Some(ctx.clone());
        self.advance_book_presentation();
        self.poll_book_open();
        self.poll_reader_navigation();
        #[cfg(debug_assertions)]
        self.check_reader_navigation(ctx);
        if self.i18n.preference() == &Language::Auto {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            if self.language_checked.elapsed() >= std::time::Duration::from_secs(1) {
                self.language_checked = std::time::Instant::now();
                if self.i18n.refresh_system_language() {
                    ctx.request_repaint();
                }
            }
        }
        self.flush_reading_position_if_due();
        self.poll_annotation_export(ctx);
        // --- Check async font discovery result ---
        if self.system_font_names.is_empty() {
            if let Ok(mut slot) = self.font_discovery_result.lock() {
                if let Some((names, paths, cjk)) = slot.take() {
                    self.system_font_names = names;
                    self.system_font_paths = paths;
                    self.cjk_font_names = cjk;
                    // Restore the user's preferred fonts now that system fonts
                    // are available, and trigger re-registration so the fonts
                    // actually get loaded into egui.
                    if let Some(preferred) = self.pending_font_family.take() {
                        if !self.reader_font_family.is_empty()
                            && self.system_font_paths.contains_key(&preferred)
                        {
                            self.reader_font_family = preferred;
                        }
                    }
                    if let Some(preferred) = self.pending_cjk_font_family.take() {
                        if self.system_font_paths.contains_key(&preferred) {
                            self.reader_cjk_font_family = preferred;
                        }
                    }
                    self.embedded_fonts_registered = false;
                }
            }
        }


        // --- Poll startup update check result ---
        if matches!(self.update_state, UpdateState::Checking) {
            // Startup checks have no GUI context in their worker. Keep polling even
            // when the system-language heartbeat is disabled by an explicit locale.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            if let Some(ref slot) = self._update_check_slot {
                if let Ok(s) = slot.lock() {
                    if let Some(ref state) = *s {
                        match state {
                            UpdateState::Available(tag) => {
                                self.push_feedback_log(format!(
                                    "[Update] new version available: {}",
                                    tag
                                ));
                                self.update_latest_tag = Some(tag.clone());
                                self.show_update_dialog = true;
                                self.update_state = UpdateState::Available(tag.clone());
                            }
                            UpdateState::CheckFailed(message) => {
                                self.push_feedback_log(format!("[Update] check failed: {message}"));
                                self.update_state = state.clone();
                            }
                            _ => {
                                self.push_feedback_log("[Update] app is up to date");
                                self.update_state = state.clone();
                            }
                        }
                        // drop will happen, but clear slot next frame
                    }
                }
                if !matches!(self.update_state, UpdateState::Checking) {
                    self._update_check_slot = None;
                }
            }
        }

        // --- Poll download result (if downloading in background) ---
        if matches!(self.update_state, UpdateState::Downloading) {
            if let Some(ref slot) = self._update_download_slot {
                if let Ok(s) = slot.lock() {
                    if let Some(ref state) = *s {
                        self.update_state = state.clone();
                    }
                }
                if !matches!(self.update_state, UpdateState::Downloading) {
                    self._update_download_slot = None;
                    self._update_progress = None;
                }
            }
        }

        // --- Startup update available dialog ---
        if self.show_update_dialog {
            self.render_update_dialog(ctx);
        }

        self.defer_custom_font_for_frame = false;

        self.appearance_transition
            .begin(ctx, (self.dark_mode, self.appearance_current_dark));
        crate::ui::design::apply(ctx, self.dark_mode);
        if self.view != AppView::Reader
            || self.scroll_mode
            || self.reader_page_animation != "Realistic"
        {
            self.page_curl.clear();
        }
        if self.view != AppView::Reader {
            self.render_app_navigation(ctx);
        }

        // Register embedded fonts from the current book
        if !self.embedded_fonts_registered {
            self.embedded_fonts_registered = true;
            let mut fonts = egui::FontDefinitions::default();

            // Save default proportional chain (egui built-in Latin fonts) before CJK additions
            let default_proportional = fonts
                .families
                .get(&egui::FontFamily::Proportional)
                .cloned()
                .unwrap_or_default();
            let default_monospace = fonts
                .families
                .get(&egui::FontFamily::Monospace)
                .cloned()
                .unwrap_or_default();

            let mut selected_latin_bound = matches!(
                self.reader_font_family.as_str(),
                "" | "Sans" | "Serif" | "Monospace"
            );
            let mut selected_cjk_bound = matches!(self.reader_cjk_font_family.as_str(), "Sans");

            // Re-register system fonts (same as setup_fonts in main.rs)
            let cjk_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\msyh.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/System/Library/Fonts/PingFang.ttc",
            ];
            for path in cjk_paths {
                if let Some(data) = crate::system_fonts::load_path(path) {
                    fonts.font_data.insert("cjk_font".to_owned(), data);
                    fonts
                        .families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .push("cjk_font".to_owned());
                    fonts
                        .families
                        .entry(egui::FontFamily::Monospace)
                        .or_default()
                        .push("cjk_font".to_owned());
                    break;
                }
            }
            let bold_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\msyhbd.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
                "/System/Library/Fonts/PingFang.ttc",
            ];
            for path in bold_paths {
                if let Some(data) = crate::system_fonts::load_path(path) {
                    fonts.font_data.insert("cjk_bold".to_owned(), data);
                    fonts.families.insert(
                        egui::FontFamily::Name("Bold".into()),
                        vec!["cjk_bold".to_owned()],
                    );
                    break;
                }
            }
            let serif_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\simsun.ttc",
                "/usr/share/fonts/truetype/noto/NotoSerif-Regular.ttf",
                "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
                "/System/Library/Fonts/Times.ttc",
            ];
            for path in serif_paths {
                if let Some(data) = crate::system_fonts::load_path(path) {
                    fonts.font_data.insert("serif_font".to_owned(), data);
                    fonts.families.insert(
                        egui::FontFamily::Name("Serif".into()),
                        vec!["serif_font".to_owned(), "cjk_font".to_owned()],
                    );
                    break;
                }
            }

            #[cfg(target_os = "macos")]
            crate::system_fonts::register_defaults(&mut fonts);
            // Rebuilding for a book must retain the startup families even when
            // an optional system face is missing on Linux or Windows.
            crate::register_reader_font_families(&mut fonts);

            // Register embedded EPUB fonts
            if let Some(book) = &self.book {
                for (name, data) in &book.fonts {
                    fonts.font_data.insert(
                        name.clone(),
                        egui::FontData::from_owned(data.clone()).into(),
                    );
                    let mut fallback = vec![name.clone()];
                    if fonts.font_data.contains_key("cjk_font") {
                        fallback.push("cjk_font".to_owned());
                    }
                    fonts
                        .families
                        .insert(egui::FontFamily::Name(name.clone().into()), fallback);
                    if *name == self.reader_font_family {
                        selected_latin_bound = true;
                    }
                    if *name == self.reader_cjk_font_family {
                        selected_cjk_bound = true;
                    }
                }
            }

            // Register custom Latin system font
            if !matches!(
                self.reader_font_family.as_str(),
                "" | "Sans" | "Serif" | "Monospace"
            ) {
                if let Some(path) = self.system_font_paths.get(&self.reader_font_family) {
                    if let Some(data) = path.load() {
                        fonts
                            .font_data
                            .insert(self.reader_font_family.clone(), data);
                        let mut fallback = vec![self.reader_font_family.clone()];
                        if fonts.font_data.contains_key("cjk_font") {
                            fallback.push("cjk_font".to_owned());
                        }
                        fonts.families.insert(
                            egui::FontFamily::Name(self.reader_font_family.clone().into()),
                            fallback,
                        );
                        selected_latin_bound = true;
                    }
                }
            }

            // Register custom title system font (falls back to the body chain).
            if !self.reader_title_font_family.trim().is_empty()
                && !matches!(
                    self.reader_title_font_family.as_str(),
                    "Sans" | "Serif" | "Monospace"
                )
                && !fonts.font_data.contains_key(&self.reader_title_font_family)
            {
                if let Some(path) = self.system_font_paths.get(&self.reader_title_font_family) {
                    if let Some(data) = path.load() {
                        fonts
                            .font_data
                            .insert(self.reader_title_font_family.clone(), data);
                        let mut fallback = vec![self.reader_title_font_family.clone()];
                        if fonts.font_data.contains_key("cjk_font") {
                            fallback.push("cjk_font".to_owned());
                        }
                        fonts.families.insert(
                            egui::FontFamily::Name(self.reader_title_font_family.clone().into()),
                            fallback,
                        );
                    }
                }
            }

            // Register custom CJK system font
            if !matches!(self.reader_cjk_font_family.as_str(), "Sans")
                && !fonts.font_data.contains_key(&self.reader_cjk_font_family)
            {
                if let Some(path) = self.system_font_paths.get(&self.reader_cjk_font_family) {
                    if let Some(data) = path.load() {
                        fonts
                            .font_data
                            .insert(self.reader_cjk_font_family.clone(), data);
                        selected_cjk_bound = true;
                    }
                }
            }

            // Latin font fallback to Sans if not bound
            if !selected_latin_bound {
                if !self.reader_font_family.is_empty()
                    && self.reader_font_family != "Sans"
                    && self.system_font_names.is_empty()
                {
                    self.pending_font_family = Some(self.reader_font_family.clone());
                }
                self.reader_font_family = "Sans".to_string();
            }

            // CJK font fallback to Sans if not bound
            if !selected_cjk_bound {
                if !self.reader_cjk_font_family.is_empty()
                    && self.reader_cjk_font_family != "Sans"
                    && self.system_font_names.is_empty()
                {
                    self.pending_cjk_font_family = Some(self.reader_cjk_font_family.clone());
                }
                self.reader_cjk_font_family = "Sans".to_string();
            }

            // Emoji / symbol fallback font
            let emoji_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\seguisym.ttf",
                "C:\\Windows\\Fonts\\seguiemj.ttf",
                "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
                "/System/Library/Fonts/Apple Color Emoji.ttc",
            ];
            for path in emoji_paths {
                if let Some(data) = crate::system_fonts::load_path(path) {
                    fonts.font_data.insert("emoji_font".to_owned(), data);
                    fonts
                        .families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .push("emoji_font".to_owned());
                    fonts
                        .families
                        .entry(egui::FontFamily::Monospace)
                        .or_default()
                        .push("emoji_font".to_owned());
                    break;
                }
            }

            // Build separate Latin-first and CJK-first reader families. egui picks the first
            // font containing each glyph, so one shared chain lets Latin fonts consume CJK
            // punctuation before the user's selected CJK font gets a chance to render it.
            let mut reader_latin_fonts = Vec::new();
            match self.reader_font_family.as_str() {
                "Sans" => {
                    extend_unique_font_chain(&mut reader_latin_fonts, default_proportional.clone())
                }
                "Serif" => {
                    if fonts.font_data.contains_key("serif_font") {
                        reader_latin_fonts.push("serif_font".to_owned());
                    }
                    extend_unique_font_chain(&mut reader_latin_fonts, default_proportional.clone());
                }
                "Monospace" => {
                    extend_unique_font_chain(&mut reader_latin_fonts, default_monospace.clone());
                }
                other => {
                    if fonts.font_data.contains_key(other) {
                        reader_latin_fonts.push(other.to_owned());
                    }
                    extend_unique_font_chain(&mut reader_latin_fonts, default_proportional.clone());
                }
            }

            let mut reader_cjk_fonts = Vec::new();
            match self.reader_cjk_font_family.as_str() {
                "Sans" => {
                    if fonts.font_data.contains_key("cjk_font") {
                        reader_cjk_fonts.push("cjk_font".to_owned());
                    }
                }
                other => {
                    if fonts.font_data.contains_key(other) {
                        reader_cjk_fonts.push(other.to_owned());
                    }
                    if fonts.font_data.contains_key("cjk_font") {
                        reader_cjk_fonts.push("cjk_font".to_owned());
                    }
                }
            }

            let mut emoji_fallback = Vec::new();
            if fonts.font_data.contains_key("emoji_font") {
                emoji_fallback.push("emoji_font".to_owned());
            }

            // An empty Latin selection uses the CJK face's own Latin glyphs.
            // Keep the normal Latin chain afterward for missing characters.
            let mut reader_font_chain = if self.reader_font_family.is_empty() {
                let mut chain = reader_cjk_fonts.clone();
                extend_unique_font_chain(&mut chain, reader_latin_fonts.clone());
                chain
            } else {
                let mut chain = reader_latin_fonts.clone();
                extend_unique_font_chain(&mut chain, reader_cjk_fonts.clone());
                chain
            };
            extend_unique_font_chain(&mut reader_font_chain, emoji_fallback.clone());

            let mut reader_cjk_chain = reader_cjk_fonts.clone();
            extend_unique_font_chain(&mut reader_cjk_chain, reader_latin_fonts.clone());
            extend_unique_font_chain(&mut reader_cjk_chain, emoji_fallback.clone());

            fonts.families.insert(
                egui::FontFamily::Name("ReaderFont".into()),
                reader_font_chain.clone(),
            );
            let mut reader_title_chain = Vec::new();
            let title_font = self.reader_title_font_family.trim();
            if !title_font.is_empty() {
                let available = fonts
                    .families
                    .contains_key(&egui::FontFamily::Name(title_font.to_string().into()));
                if available || matches!(title_font, "Sans" | "Serif" | "Monospace") {
                    reader_title_chain.push(title_font.to_string());
                }
            }
            extend_unique_font_chain(&mut reader_title_chain, reader_font_chain.clone());
            fonts.families.insert(
                egui::FontFamily::Name("ReaderTitleFont".into()),
                reader_title_chain,
            );
            crate::ui::reader_block::set_title_font_ready(true);

            fonts
                .families
                .insert(egui::FontFamily::Name("ReaderCjk".into()), reader_cjk_chain);

            crate::system_fonts::register_fallbacks(&mut fonts);
            ctx.set_fonts(fonts);
            self.pages_dirty = true;
            self.defer_custom_font_for_frame = true;
            ctx.request_repaint();
        }

        self.handle_reader_shortcuts(ctx);

        if let Some(err) = self.error_msg.clone() {
            let error_title = self.i18n.t("error.title").to_string();
            let error_ok = self.i18n.t("error.ok").to_string();
            egui::Window::new(error_title)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(&err);
                    if ui.button(&error_ok).clicked() {
                        self.error_msg = None;
                    }
                });
        }

        ctx.data_mut(|d| d.insert_temp(egui::Id::new("panels_animating"), false));
        if self.view == AppView::Reader {
            self.ensure_reader_bg_texture(ctx);

            let collapse_label = self.i18n.t("common.collapse").to_owned();
            if crate::ui::design::animated_side_panel(
                ctx,
                "toc_panel",
                true,
                280.,
                self.show_toc && self.book.is_some(),
                &collapse_label,
                |ui| self.render_toc(ui),
            ) {
                self.show_toc = false;
            }
        }

        let reader_fill = if self.view == AppView::Reader {
            self.reader_bg_color
        } else {
            crate::ui::design::Palette::new(self.dark_mode).canvas
        };

        // ── Side-panels (rendered before CentralPanel) ──
        self.render_search_panel(ctx);
        self.render_annotations_panel(ctx);
        self.render_settings_panel(ctx);
        self.render_notes_search(ctx);
        self.render_note_popup(ctx);
        self.render_quick_open(ctx);
        self.render_bookmark_toast(ctx);

        // ── Review Panel ──
        self.render_review_panel(ctx);

        let panels_moving = crate::ui::design::panels_animating(ctx);
        if panels_moving && self.panel_reflow_anchor.is_none() {
            self.panel_reflow_anchor = Some((self.current_chapter, self.current_block));
            self.page_curl.clear();
        }
        egui::CentralPanel::default()
            .frame(if self.view == AppView::Reader {
                egui::Frame::NONE.fill(reader_fill)
            } else {
                egui::Frame::default().fill(reader_fill)
            })
            .show(ctx, |ui| match self.view {
                AppView::Library => self.render_library(ui),
                AppView::Notes => self.render_notes_page(ui),
                AppView::Reader => self.render_reader(ui),
            });

        // Chrome floats over the paper; it never reserves space in pagination.
        self.render_reader_chrome(ctx);

        if !panels_moving {
            self.panel_reflow_anchor = None;
        }

        // ── Floating windows ──
        self.render_appearance_window(ctx);

        // ── About Window ──
        if self.show_about {
            self.render_about(ctx);
        }

        self.render_display_reset_confirmation(ctx);
        self.render_language_picker(ctx);

        // ── TXT Import ──
        if self.txt_import.is_some() {
            self.render_txt_import(ctx);
        }

        self.render_book_opening(ctx);
        self.render_reader_introduction(ctx);
        #[cfg(debug_assertions)]
        self.capture_appearance_preview(ctx);
        #[cfg(feature = "performance-checks")]
        if let Some(mut probe) = self.reader_performance.take() {
            if !probe.tick(self, ctx, performance_frame.elapsed().as_secs_f64() * 1000.) {
                self.reader_performance = Some(probe);
            }
        }
        #[cfg(feature = "performance-checks")]
        if let Some(mut probe) = self.shelf_performance.take() {
            if !probe.tick(self, ctx, performance_frame.elapsed().as_secs_f64() * 1000.) {
                self.shelf_performance = Some(probe);
            }
        }
        #[cfg(feature = "performance-checks")]
        if let Some(mut probe) = self.native_corpus.take() {
            if !probe.tick(self, ctx) {
                self.native_corpus = Some(probe);
            }
        }
        self.appearance_transition
            .finish(ctx, self.reader_bg_texture.clone());
        if self.view == AppView::Reader {
            let rect = self.paging_page_rect.unwrap_or_else(|| ctx.screen_rect());
            self.reader_navigation
                .finish_frame(ctx, rect, self.reader_bg_texture.clone());
        }
        let settings = AppSettings::from_app(self);
        if self.last_saved_settings.as_ref() != Some(&settings) {
            if let Err(error) = settings.save(&self.data_dir) {
                self.error_msg = Some(error);
            }
            self.last_saved_settings = Some(settings);
        }
    }
}

fn default_scripts_enabled() -> bool {
    true
}

#[cfg(test)]
mod first_reader_preferences_tests {
    use super::*;
    #[test]
    fn closing_a_pending_open_stops_its_worker_and_rejects_its_late_result() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(dir.path().to_string_lossy().into());
        let token = reader_core::open_operation::Cancellation::default();
        let (tx, rx) = std::sync::mpsc::channel();
        app.pending_book_open = Some(("cancelled.epub".into(), None, rx));
        app.pending_open_cancel = Some(token.clone());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = token.run(|| {
                ready_tx.send(()).unwrap();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                loop {
                    reader_core::open_operation::check()?;
                    if std::time::Instant::now() > deadline {
                        return Err("Cancellation was not observed".into());
                    }
                    std::thread::yield_now();
                }
            });
            assert_eq!(
                result.as_ref().err().map(String::as_str),
                Some(reader_core::open_operation::CANCELLED)
            );
            let _ = tx.send(result);
        });
        ready_rx.recv().unwrap();
        app.close_book_presentation();
        worker.join().unwrap();
        app.poll_book_open();
        assert!(app.pending_book_open.is_none());
        assert!(app.pending_open_cancel.is_none());
        assert!(app.book.is_none());
        assert!(app.view == AppView::Library);
    }
    #[test]
    fn corrupt_catalog_is_reported_and_a_failed_reload_keeps_the_visible_shelf() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        std::fs::write(dir.path().join("library.json"), b"damaged").unwrap();
        let mut app = ReaderApp::from_data_dir(root);
        assert!(app
            .error_msg
            .as_ref()
            .unwrap()
            .contains("Invalid stored data"));
        app.library.books.push(reader_core::library::BookEntry {
            id: "00000000-0000-4000-8000-000000000001".into(),
            title: "Keep visible".into(),
            path: "book.epub".into(),
            last_chapter: 0,
            last_block: 0,
            last_char_offset: 0,
            last_chapter_title: None,
            last_opened: 0,
        });
        app.reload_library();
        assert_eq!(app.library.books.len(), 1);
        assert_eq!(
            std::fs::read(dir.path().join("library.json")).unwrap(),
            b"damaged"
        );
    }

    #[test]
    fn single_column_preference_persists_and_legacy_settings_default_to_auto() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let mut app = ReaderApp::from_data_dir(path.clone());
        assert!(!app.force_single_column);
        app.force_single_column = true;
        app.font_size = 23.;
        let settings = AppSettings::from_app(&app);
        settings.save(&path).unwrap();
        let restored = ReaderApp::from_data_dir(path);
        assert!(restored.force_single_column);
        assert_eq!(restored.font_size, 23.);
        let mut legacy = serde_json::to_value(settings).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("force_single_column");
        let legacy: AppSettings = serde_json::from_value(legacy).unwrap();
        assert!(!legacy.force_single_column);
        assert_eq!(legacy.font_size, 23.);
    }
    #[test]
    fn first_run_defaults_and_previous_reader_migration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_string_lossy().into_owned();
        let fresh = ReaderApp::from_data_dir(path.clone());
        assert!(!fresh.scroll_mode);
        assert_eq!(fresh.reader_page_animation, "Realistic");
        assert!(!fresh.reader_introduction_seen);
        let mut saved = AppSettings::from_app(&fresh);
        saved.scroll_mode = true;
        saved.reader_page_animation = "Slide".into();
        saved.reader_introduction_seen = Some(true);
        saved.save(&path).unwrap();
        let restored = ReaderApp::from_data_dir(path.clone());
        assert!(restored.scroll_mode && restored.reader_introduction_seen);
        assert_eq!(restored.reader_page_animation, "Slide");
        let mut legacy = serde_json::to_value(saved).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("reader_introduction_seen");
        legacy["last_book_path"] = serde_json::json!("/missing/previous-reading.epub");
        std::fs::write(
            AppSettings::path(&path),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        let upgraded = ReaderApp::from_data_dir(path);
        assert!(upgraded.reader_introduction_seen && upgraded.scroll_mode);
        assert_eq!(upgraded.reader_page_animation, "Slide");
        legacy["reader_introduction_seen"] = serde_json::json!("malformed");
        let decoded: AppSettings = serde_json::from_value(legacy).unwrap();
        assert_eq!(decoded.reader_introduction_seen, None);
        assert!(decoded.scroll_mode);
        assert_eq!(decoded.reader_page_animation, "Slide");
    }
}

#[cfg(test)]
mod reading_theme_settings_tests {
    use super::*;
    use crate::ui::reading_themes::{template, ReadingThemes};

    #[test]
    fn themes_persist_active_edits_without_touching_other_slots_or_reading_position() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let mut app = ReaderApp::from_data_dir(path.clone());
        let mut custom = template("custom").unwrap();
        custom.dark.image = Some("/external/custom.png".into());
        app.reading_themes.save("custom", custom.clone());
        let mut warm = template("warm").unwrap();
        warm.dark.ink = Some([210, 211, 212, 255]);
        app.reading_themes.save("warm", warm.clone());
        app.font_size = 23.;
        app.line_spacing = 1.9;
        app.current_chapter = 7;
        app.current_block = 3;
        app.current_char_offset = 5;
        app.apply_selected_reading_theme();
        assert_eq!(
            (
                app.current_chapter,
                app.current_block,
                app.current_char_offset
            ),
            (7, 3, 5)
        );
        assert_eq!((app.font_size, app.line_spacing), (23., 1.9));
        app.reader_bg_color = Color32::from_rgb(11, 22, 33);
        let settings = AppSettings::from_app(&app);
        let themes = settings.reading_themes.as_ref().unwrap();
        assert_eq!(themes.selected, "warm");
        assert_eq!(themes.custom.as_ref(), Some(&custom));
        assert_eq!(themes.pair("warm").dark, warm.dark);
        assert_eq!(themes.pair("warm").light.paper, [11, 22, 33, 255]);
        settings.save(&path).unwrap();
        let restored = ReaderApp::from_data_dir(path);
        assert_eq!(&restored.reading_themes, themes);
        assert_eq!(restored.reader_bg_color, Color32::from_rgb(11, 22, 33));
        assert_eq!((restored.font_size, restored.line_spacing), (23., 1.9));
    }

    #[test]
    fn missing_or_malformed_new_field_preserves_legacy_pair_and_unrelated_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let app = ReaderApp::from_data_dir(path.clone());
        let mut settings = AppSettings::from_app(&app);
        let mut pair = template("gray").unwrap();
        pair.light.image = Some("/external/old-light.png".into());
        pair.dark.image = Some("/external/old-dark.png".into());
        pair.dark.strength = 0.61;
        pair.light.ink = Some([41, 42, 43, 255]);
        settings.appearance_paints = Some(pair.clone());
        settings.reading_themes = None;
        settings.appearance_current_dark = Some(false);
        settings.font_size = 27.;
        settings.scroll_mode = true;
        for malformed in [false, true] {
            let mut raw = serde_json::to_value(&settings).unwrap();
            if malformed {
                raw["reading_themes"] =
                    serde_json::json!({"custom":"invalid", "selected":"custom"});
            } else {
                raw.as_object_mut().unwrap().remove("reading_themes");
            }
            std::fs::write(AppSettings::path(&path), serde_json::to_vec(&raw).unwrap()).unwrap();
            let restored = ReaderApp::from_data_dir(path.clone());
            assert_eq!(
                restored.reading_themes,
                ReadingThemes::migrate(pair.clone())
            );
            assert_eq!(restored.appearance_paints, pair);
            assert_eq!(
                restored.reader_bg_image_path.as_deref(),
                Some("/external/old-light.png")
            );
            assert_eq!(restored.font_size, 27.);
            assert!(restored.scroll_mode);
        }
    }
}

#[cfg(test)]
mod reading_navigation_tests {
    use super::*;
    use crate::ui::reader_navigation::{NavigationKind, ReaderLocation};

    fn navigation_fixture() -> (tempfile::TempDir, ReaderApp) {
        let dir = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(dir.path().to_string_lossy().into_owned());
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ios/Tests/fixtures/native-reading-modes.epub");
        app.finish_book_open(
            source.to_str().unwrap(),
            Some(19),
            Ok((EpubBook::open(&source).unwrap(), None)),
        );
        (dir, app)
    }
    #[test]
    fn reopening_normalizes_an_out_of_range_legacy_chapter_without_resetting_valid_locations() {
        let (_dir, mut app) = navigation_fixture();
        let path = app.book_path.clone().unwrap();
        app.library
            .update_position(&app.data_dir, &path, 9999, None, 99, 99)
            .unwrap();
        let book = EpubBook::open(&path).unwrap();
        let last = book.chapters.len() - 1;
        app.finish_book_open(&path, Some(0), Ok((book, None)));
        assert!(app.error_msg.is_none(), "{:?}", app.error_msg);
        assert_eq!(
            app.reader_location(),
            ReaderLocation {
                chapter: last,
                block: 0,
                char_offset: 0
            }
        );
        let stored = reader_core::library::Library::load_from(&app.data_dir).unwrap();
        assert_eq!(
            (stored.books[0].last_chapter, stored.books[0].last_block),
            (last, 0)
        );
        app.library
            .update_position(&app.data_dir, &path, 19, None, 3, 37)
            .unwrap();
        app.finish_book_open(&path, Some(0), Ok((EpubBook::open(&path).unwrap(), None)));
        assert_eq!(
            app.reader_location(),
            ReaderLocation {
                chapter: 19,
                block: 3,
                char_offset: 37
            }
        );
    }

    #[test]
    fn jump_return_and_restart_preserve_exact_coordinates() {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().to_string_lossy().into_owned();
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ios/Tests/fixtures/native-reading-modes.epub");
        let path = source.to_str().unwrap();
        let mut app = ReaderApp::from_data_dir(data.clone());
        app.finish_book_open(path, Some(19), Ok((EpubBook::open(path).unwrap(), None)));
        let origin = ReaderLocation {
            chapter: 19,
            block: 3,
            char_offset: 37,
        };
        app.current_chapter = origin.chapter;
        app.current_block = origin.block;
        app.current_char_offset = origin.char_offset;
        app.current_page = 2;
        app.total_pages = 9;
        let target = ReaderLocation {
            chapter: 21,
            block: 3,
            char_offset: 3,
        };
        app.navigate_reader(target, NavigationKind::Local);
        for _ in 0..500 {
            app.poll_reader_navigation();
            if app.reader_location() == target {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(app.reader_location(), target);
        assert_eq!(app.reader_navigation.history.len(), 1);
        assert_eq!(app.reader_navigation.history[0].location, origin);
        assert!(app.reader_navigation.history[0].page.contains('3'));
        app.load_reader_navigation();
        assert_eq!(app.reader_navigation.history[0].location, origin);
        app.navigate_reader(origin, NavigationKind::Return);
        for _ in 0..500 {
            app.poll_reader_navigation();
            if app.reader_location() == origin {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(app.reader_location(), origin);
        assert!(app.reader_navigation.history.is_empty());
        app.schedule_position_save_at(19, 3, 37);
        drop(app);
        let mut restored = ReaderApp::from_data_dir(data.clone());
        for _ in 0..500 {
            restored.poll_book_open();
            if restored.book.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(restored.reader_location(), origin);
        assert!(restored.book.is_some());
        restored.close_book_presentation();
        assert!(AppSettings::from_app(&restored).last_book_path.is_none());
    }
    #[test]
    fn batch_import_keeps_valid_books_and_reports_bad_inputs_without_opening_them() {
        let dir = tempfile::tempdir().unwrap();
        let input = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for name in ["One", "Two"] {
            let txt = input.path().join(format!("{name}.txt"));
            std::fs::write(&txt, format!("第一章 {name}\n{name} body text")).unwrap();
            paths.push(
                reader_core::txt::convert_txt_to_epub(&txt, input.path(), &Default::default())
                    .unwrap()
                    .epub_path,
            );
        }
        paths.push(paths[0].clone());
        let invalid = input.path().join("bad.epub");
        std::fs::write(&invalid, b"not an epub").unwrap();
        paths.push(invalid);
        let txt = input.path().join("manual.txt");
        std::fs::write(&txt, b"body").unwrap();
        paths.push(txt);
        let mut app = ReaderApp::from_data_dir(dir.path().to_string_lossy().into_owned());
        app.import_epub_batch(paths);
        for _ in 0..500 {
            app.poll_book_open();
            if app.pending_batch_import.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(app.pending_batch_import.is_none());
        assert_eq!(app.library.books.len(), 2);
        assert!(app.book.is_none());
        assert!(app
            .error_msg
            .as_ref()
            .is_some_and(|e| e.contains("bad.epub") && e.contains("manual.txt")));
        assert!(app.library.books.iter().all(|b| b.last_opened == 0));
    }
}
