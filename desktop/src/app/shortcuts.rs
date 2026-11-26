//! Keyboard dispatch stays behind the reader's focus and overlay guards.
use super::{keymap, AppView, ReaderApp};
use eframe::egui;

const LABEL_KEYS: [char; 20] = ['a','s','d','f','z','x','c','q','w','e','r','t','g','b','y','u','i','o','p','n'];

impl ReaderApp {
    fn shortcut(&self, name: &str) -> String {
        match self.shortcuts.get(name) {
            Some(value) if !value.trim().is_empty() => value.clone(),
            _ => super::default_shortcuts()
                .get(name)
                .cloned()
                .unwrap_or_default(),
        }
    }

    pub(super) fn handle_reader_shortcuts(&mut self, ctx: &egui::Context) {
        let blocked = self.show_reader_introduction
            || self.show_review_panel
            || self.show_language;

        if let Some(action) = self.shortcut_capture.clone() {
            if let Some(spec) = keymap::capture(ctx) {
                if spec.is_empty() {
                    self.shortcut_capture = None;
                } else {
                    self.shortcuts.insert(action, spec);
                    self.shortcut_capture = None;
                }
            }
            return;
        }

        let (k_open, k_library, k_annotations, k_search, k_toc, k_settings, k_highlight, k_undo, k_export, k_notes_search) = (
            self.shortcut("open"),
            self.shortcut("library"),
            self.shortcut("annotations"),
            self.shortcut("search"),
            self.shortcut("toc"),
            self.shortcut("settings"),
            self.shortcut("highlight"),
            self.shortcut("undo_highlight"),
            self.shortcut("export_notes"),
            self.shortcut("notes_search"),
        );
        let k_notes_page = self.shortcut("notes_page");
        let k_tag_focus = self.shortcut("tag_focus");
        let k_bookmark = self.shortcut("bookmark");
        let k_quick_open = self.shortcut("quick_open");
        let k_vim = self.shortcut("vim_mode");
        let k_font_smaller = self.shortcut("font_smaller");
        let k_font_larger = self.shortcut("font_larger");

        // 有选区或笔记弹窗时不吞 Ctrl+A/H
        if self.note_popup.is_some() || self.text_selection.is_some() {
            if !k_highlight.is_empty() && keymap::pressed(ctx, &k_highlight) {
                self.note_popup = None;
                self.highlight_current_block();
                return;
            }
        }

        // 笔记弹窗：Esc 取消 / Ctrl+Enter 保存（不依赖 TextEdit 是否吞键）
        if self.note_popup.is_some() {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.note_popup = None;
                return;
            }
            if ctx.input(|i| (i.modifiers.ctrl || i.modifiers.command) && i.key_pressed(egui::Key::Enter)) {
                self.save_note_popup_current();
                return;
            }
        }

        if self.view == AppView::Notes {
            if !k_highlight.is_empty() && keymap::pressed(ctx, &k_highlight) {
                let total = self.notes_page.rows.len();
                if self.notes_page.selected.len() >= total && total > 0 {
                    self.notes_page.selected.clear();
                } else {
                    self.notes_page.selected = self
                        .notes_page
                        .rows
                        .iter()
                        .map(|r| format!("{}:{}", r.book_id, r.id))
                        .collect();
                }
                return;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
                if !self.notes_page.selected.is_empty() {
                    self.notes_page.confirm_focus = true;
                    self.notes_page.confirm_batch = self
                        .notes_page
                        .selected
                        .iter()
                        .filter_map(|k| k.split_once(':').map(|(b, h)| (b.to_string(), h.to_string())))
                        .collect();
                }
                return;
            }
        }

        // Esc 不受输入框焦点影响：先关弹窗/面板，笔记页返回阅读
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.note_popup.take().is_some() {
                return;
            }
            if self.show_annotations {
                self.show_annotations = false;
                return;
            }
            if self.view == AppView::Notes {
                if !self.notes_page.confirm_batch.is_empty() {
                    self.notes_page.confirm_batch.clear();
                    return;
                }
                if self.notes_page.confirm.is_some() {
                    self.notes_page.confirm = None;
                    return;
                }
                if self.notes_page.tag_target.is_some() {
                    self.notes_page.tag_target = None;
                    return;
                }
                self.view = if self.book.is_some() {
                    AppView::Reader
                } else {
                    AppView::Library
                };
                return;
            }
        }
        if !k_quick_open.is_empty() && keymap::pressed(ctx, &k_quick_open) {
            self.quick_open.open = !self.quick_open.open;
            self.quick_open.query.clear();
            self.quick_open.selected = 0;
            self.quick_open.focus = true;
            return;
        }
        if self.quick_open.open {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.quick_open.open = false;
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                let n = self.quick_open_entries().len();
                if n > 0 {
                    self.quick_open.selected = (self.quick_open.selected + 1).min(n - 1);
                }
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                self.quick_open.selected = self.quick_open.selected.saturating_sub(1);
            } else if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                let entries = self.quick_open_entries();
                if let Some(idx) = entries.get(self.quick_open.selected).copied() {
                    if let Some(book) = self.library.books.get(idx).cloned() {
                        self.quick_open.open = false;
                        self.open_book_from_path(&book.path, Some(book.last_chapter));
                    }
                }
            }
            return;
        }
        // Ctrl+K 后 Ctrl+S（顺序也可颠倒）直接打开快捷键设置。
        if !blocked {
            let ctrl = ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);
            if ctrl {
                let now = std::time::Instant::now();
                let pressed = |key| ctx.input(|i| i.key_pressed(key));
                let mut open = false;
                if pressed(egui::Key::K) {
                    open = self
                        .shortcut_chord
                        .take()
                        .is_some_and(|(first, at)| {
                            first == 's' && at.elapsed() < std::time::Duration::from_secs(2)
                        });
                    if !open {
                        self.shortcut_chord = Some(('k', now));
                    }
                } else if pressed(egui::Key::S) {
                    open = self
                        .shortcut_chord
                        .take()
                        .is_some_and(|(first, at)| {
                            first == 'k' && at.elapsed() < std::time::Duration::from_secs(2)
                        });
                    if !open {
                        self.shortcut_chord = Some(('s', now));
                    }
                }
                if open {
                    self.shortcut_chord = None;
                    self.show_settings = true;
                    self.settings_tab = crate::app::SettingsTab::Shortcuts;
                    self.show_annotations = false;
                    self.show_search = false;
                    return;
                }
            }
        }
        if blocked || ctx.wants_keyboard_input() || ctx.memory(|m| m.any_popup_open()) {
            return;
        }

        // ── Esc：先关笔记弹窗，再关面板 ──
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.note_popup.take().is_some() {
                return;
            }
            if self.show_annotations {
                self.show_annotations = false;
                return;
            }
            if self.show_settings {
                self.show_settings = false;
                return;
            }
        }

        if !k_open.is_empty() && keymap::pressed(ctx, &k_open) {
            if self.view == AppView::Reader {
                self.close_book_presentation();
            }
            self.open_file_dialog();
            return;
        }
        if !k_library.is_empty() && keymap::pressed(ctx, &k_library) {
            if self.view == AppView::Reader {
                self.close_book_presentation();
                self.show_annotations = false;
                self.show_settings = false;
                self.show_search = false;
            }
            return;
        }
        if !k_bookmark.is_empty() && keymap::pressed(ctx, &k_bookmark) {
            if self.view == AppView::Reader {
                if let Some(mut cfg) = self.book_config.clone() {
                    let chapter = self.current_chapter;
                    let block = self.current_block;
                    let existed = cfg
                        .bookmarks
                        .iter()
                        .any(|b| b.chapter == chapter && b.block == block);
                    if existed {
                        cfg.bookmarks
                            .retain(|b| !(b.chapter == chapter && b.block == block));
                    } else {
                        cfg.bookmarks.push(reader_core::library::Bookmark {
                            chapter,
                            block,
                            created_at: reader_core::now_secs(),
                        });
                    }
                    if cfg.save(&self.data_dir).is_ok() {
                        self.book_config = Some(cfg);
                        let msg = if existed { "已取消书签" } else { "已添加书签" };
                        let until = ctx.input(|i| i.time) + 1.2;
                        self.bookmark_toast = Some((msg.to_string(), until));
                    }
                }
            }
            return;
        }
        if !k_tag_focus.is_empty() && keymap::pressed(ctx, &k_tag_focus) {
            if let Some(popup) = self.note_popup.as_mut() {
                popup.tag_focus = true;
            } else if self.view == AppView::Reader {
                self.show_annotations = true;
                self.annotations_panel.tab = Some(2);
            }
            return;
        }
        if !k_export.is_empty() && keymap::pressed(ctx, &k_export) {
            if self.view == AppView::Reader {
                self.start_annotation_export(ctx);
            } else if self.view == AppView::Notes {
                self.export_notes_markdown();
            }
            return;
        }
        if !k_undo.is_empty() && keymap::pressed(ctx, &k_undo) {
            self.undo_last_highlight();
            return;
        }
        if !k_notes_page.is_empty() && keymap::pressed(ctx, &k_notes_page) {
            self.view = AppView::Notes;
            self.notes_page.dirty = true;
            self.notes_page.focus = true;
            return;
        }
        if !k_notes_search.is_empty() && keymap::pressed(ctx, &k_notes_search) {
            self.show_notes_search = !self.show_notes_search;
            self.notes_search_just_opened = self.show_notes_search;
            return;
        }
        if !k_settings.is_empty() && keymap::pressed(ctx, &k_settings) {
            self.show_settings = !self.show_settings;
            self.show_annotations = false;
            self.show_search = false;
            return;
        }

        if self.view == AppView::Reader {
            if !k_annotations.is_empty() && keymap::pressed(ctx, &k_annotations) {
                self.show_annotations = !self.show_annotations;
                self.show_settings = false;
                self.show_search = false;
                return;
            }
            if !k_search.is_empty() && keymap::pressed(ctx, &k_search) {
                self.show_search = !self.show_search;
                if self.show_search {
                    self.search_focus = true;
                }
                self.show_annotations = false;
                self.show_settings = false;
                return;
            }
            if !k_toc.is_empty() && keymap::pressed(ctx, &k_toc) {
                self.show_toc = !self.show_toc;
                return;
            }
            if !k_vim.is_empty() && keymap::pressed(ctx, &k_vim) {
                self.vim.toggle();
                return;
            }
            if !k_font_smaller.is_empty() && keymap::pressed(ctx, &k_font_smaller) {
                self.font_size = (self.font_size - 1.).clamp(12., 40.);
                self.pages_dirty = true;
                return;
            }
            if !k_font_larger.is_empty() && keymap::pressed(ctx, &k_font_larger) {
                self.font_size = (self.font_size + 1.).clamp(12., 40.);
                self.pages_dirty = true;
                return;
            }
            if self.vim.active() {
                self.handle_vim_keys(ctx);
                return;
            }
            if !k_highlight.is_empty() && keymap::pressed(ctx, &k_highlight) {
                self.highlight_current_block();
                return;
            }
            // 对齐 BN：H 直接高亮当前段落
            if ctx.input(|i| {
                !(i.modifiers.ctrl || i.modifiers.command || i.modifiers.alt)
                    && i.key_pressed(egui::Key::H)
            }) {
                self.highlight_current_block();
                return;
            }

            ctx.input(|i| {
                if i.key_pressed(egui::Key::ArrowLeft) {
                    if self.scroll_mode {
                        self.prev_chapter();
                    } else {
                        self.turn_reader_page(false);
                    }
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    if self.scroll_mode {
                        self.next_chapter();
                    } else {
                        self.turn_reader_page(true);
                    }
                }
            });
        }
    }

    /// 按 (start_block,start_char)-(end_block,end_char) 建高亮；成功返回 true。
    fn create_highlights(
        &mut self,
        sb: usize,
        sc: usize,
        eb: usize,
        ec: usize,
    ) -> bool {
        let Some(mut config) = self.book_config.clone() else {
            return false;
        };
        let Some(book) = self.book.as_ref() else {
            return false;
        };
        let Some(chapter) = book.chapters.get(self.current_chapter) else {
            return false;
        };
        let mut preview = String::new();
        let mut created: Option<String> = None;
        for idx in sb..=eb {
            let Some(block) = chapter.blocks.get(idx) else {
                continue;
            };
            let text: String = match block {
                reader_core::epub::ContentBlock::Heading { spans, .. }
                | reader_core::epub::ContentBlock::Paragraph { spans, .. } => {
                    spans.iter().map(|s| s.text.as_str()).collect()
                }
                _ => continue,
            };
            let len = text.chars().count();
            let start = if idx == sb { sc } else { 0 };
            let end = if idx == eb { ec.min(len) } else { len };
            if start >= end {
                continue;
            }
            let slice: String = text.chars().skip(start).take(end - start).collect();
            if let Some(existing) = config.highlights.iter().find(|h| {
                h.chapter == self.current_chapter
                    && h.start_block == idx
                    && h.start_offset == start
                    && h.end_offset == end
            }) {
                preview = slice;
                created = Some(existing.id.clone());
                continue;
            }
            let id = reader_core::annotations::new_highlight_id();
            config.highlights.push(reader_core::library::Highlight {
                id: id.clone(),
                chapter: self.current_chapter,
                start_block: idx,
                start_offset: start,
                end_block: idx,
                end_offset: end,
                color: reader_core::library::HighlightColor::Yellow,
                created_at: reader_core::now_secs(),
                tags: Vec::new(),
                text: slice.clone(),
            });
            preview.push_str(&slice);
            created = Some(id);
        }
        let Some(id) = created else {
            return true;
        };
        match config.save(&self.data_dir) {
            Ok(()) => {
                self.book_config = Some(config);
                self.last_highlight_undo = Some(id.clone());
                self.note_popup = Some(crate::ui::note_popup::NotePopup {
                    highlight_id: id,
                    buffer: String::new(),
                    focus: true,
                    preview: preview.chars().take(160).collect(),
                    tag_buf: String::new(),
                    tags: Vec::new(),
                    book_id: None,
                    tag_focus: false,
                });
                self.text_selection = None;
            }
            Err(error) => self.error_msg = Some(error),
        }
        true
    }

    fn block_text(&self, block: usize) -> Option<(String, usize)> {
        let book = self.book.as_ref()?;
        let chapter = book.chapters.get(self.current_chapter)?;
        let index = block.min(chapter.blocks.len().checked_sub(1)?);
        let text: String = match chapter.blocks.get(index)? {
            reader_core::epub::ContentBlock::Heading { spans, .. }
            | reader_core::epub::ContentBlock::Paragraph { spans, .. } => {
                spans.iter().map(|s| s.text.as_str()).collect()
            }
            _ => String::new(),
        };
        let len = text.chars().count();
        Some((text, len))
    }

    fn highlight_current_block(&mut self) {
        // 优先高亮用户当前选区（与手动高亮一致）
        if let Some(sel) = self.text_selection.clone() {
            let (sb, sc, eb, ec) = sel.normalized_range();
            if self.create_highlights(sb, sc, eb, ec) {
                return;
            }
        }
        // 无选区：取当前页第一个【正文段落】。
        // 标题块会被渲染成页面标题（不经过高亮布局），图片/分隔块没有文字。
        let page_range = self
            .page_block_ranges
            .get(self.current_page)
            .copied()
            .unwrap_or((self.current_block, self.current_block + 1));
        let block = self
            .book
            .as_ref()
            .and_then(|b| b.chapters.get(self.current_chapter))
            .map(|chapter| {
                (page_range.0..page_range.1.max(page_range.0 + 1))
                    .filter(|&i| {
                        matches!(
                            chapter.blocks.get(i),
                            Some(reader_core::epub::ContentBlock::Paragraph { spans, .. })
                                if spans.iter().any(|s| !s.text.trim().is_empty())
                        )
                    })
                    .next()
                    .unwrap_or(page_range.0)
            })
            .unwrap_or(self.current_block);
        self.current_block = block;
        let Some((text, len)) = self.block_text(block) else {
            return;
        };
        if len == 0 || text.trim().is_empty() {
            return;
        }
        let Some(mut config) = self.book_config.clone() else {
            return;
        };
        let id = reader_core::annotations::new_highlight_id();
        config.highlights.push(reader_core::library::Highlight {
            id: id.clone(),
            chapter: self.current_chapter,
            start_block: block,
            start_offset: 0,
            end_block: block,
            end_offset: len,
            color: reader_core::library::HighlightColor::Yellow,
            created_at: reader_core::now_secs(),
            tags: Vec::new(),
            text: text.clone(),
        });
        match config.save(&self.data_dir) {
            Ok(()) => {
                self.book_config = Some(config);
                self.last_highlight_undo = Some(id.clone());
                self.note_popup = Some(crate::ui::note_popup::NotePopup {
                    highlight_id: id,
                    buffer: String::new(),
                    focus: true,
                    preview: text.chars().take(160).collect(),
                    tag_buf: String::new(),
                    tags: Vec::new(),
                    book_id: None,
                    tag_focus: false,
                });
                self.text_selection = None;
            }
            Err(error) => self.error_msg = Some(error),
        }
    }

    fn handle_vim_keys(&mut self, ctx: &egui::Context) {
        use crate::ui::vim::VimMode;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.vim.mode = match self.vim.mode {
                VimMode::Visual => VimMode::Normal,
                _ => VimMode::Off,
            };
            self.vim.labels = false;
            return;
        }
        let Some(chapter) = self
            .book
            .as_ref()
            .and_then(|b| b.chapters.get(self.current_chapter))
        else {
            return;
        };
        let blocks_len = chapter.blocks.len();
        let char_len = |b: usize| -> usize {
            match chapter.blocks.get(b) {
                Some(reader_core::epub::ContentBlock::Paragraph { spans, .. })
                | Some(reader_core::epub::ContentBlock::Heading { spans, .. }) => {
                    spans.iter().map(|s| s.text.chars().count()).sum()
                }
                _ => 0,
            }
        };
        let (v_key, cap_v) = ctx.input(|i| {
            (
                !i.modifiers.shift && i.key_pressed(egui::Key::V),
                i.modifiers.shift && i.key_pressed(egui::Key::V),
            )
        });
        match self.vim.mode {
            VimMode::Normal => {
                if v_key {
                    self.vim.labels = !self.vim.labels;
                    return;
                }
                if cap_v {
                    self.vim.start = self.vim.cursor;
                    self.vim.mode = VimMode::Visual;
                    return;
                }
                let (j, k, h, l) = ctx.input(|i| {
                    (
                        i.key_pressed(egui::Key::J),
                        i.key_pressed(egui::Key::K),
                        i.key_pressed(egui::Key::H),
                        i.key_pressed(egui::Key::L),
                    )
                });
                if j && blocks_len > 0 {
                    self.vim.cursor = ((self.vim.cursor.0 + 1).min(blocks_len - 1), 0);
                } else if k {
                    self.vim.cursor = (self.vim.cursor.0.saturating_sub(1), 0);
                } else if h {
                    self.vim.cursor.1 = self.vim.cursor.1.saturating_sub(1);
                } else if l {
                    self.vim.cursor.1 = (self.vim.cursor.1 + 1).min(char_len(self.vim.cursor.0));
                } else if self.vim.labels {
                    if let Some(ch) = ctx.input(|i| {
                        LABEL_KEYS.iter().copied().find(|c| {
                            egui::Key::from_name(&c.to_string())
                                .is_some_and(|k| i.key_pressed(k))
                        })
                    }) {
                        if let Some(block) = crate::ui::reader_block::vim_label_target(ch) {
                            self.vim.cursor = (block, 0);
                        }
                    }
                }
            }
            VimMode::Visual => {
                if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::A)) {
                    self.highlight_vim_visual();
                    return;
                }
                let (j, k, h, l) = ctx.input(|i| {
                    (
                        i.key_pressed(egui::Key::J),
                        i.key_pressed(egui::Key::K),
                        i.key_pressed(egui::Key::H),
                        i.key_pressed(egui::Key::L),
                    )
                });
                if j && blocks_len > 0 {
                    self.vim.cursor.0 = (self.vim.cursor.0 + 1).min(blocks_len - 1);
                } else if k {
                    self.vim.cursor.0 = self.vim.cursor.0.saturating_sub(1);
                } else if h {
                    self.vim.cursor.1 = self.vim.cursor.1.saturating_sub(1);
                } else if l {
                    self.vim.cursor.1 = (self.vim.cursor.1 + 1).min(char_len(self.vim.cursor.0));
                }
            }
            VimMode::Off => {}
        }
    }

    fn highlight_vim_visual(&mut self) {
        let (sb, sc) = self.vim.start;
        let (eb, ec) = self.vim.cursor;
        let (lo, hi) = (sb.min(eb), sb.max(eb));
        let (cs, ce) = if lo == hi {
            (sc.min(ec), sc.max(ec))
        } else if lo == sb {
            (sc, ec)
        } else {
            (ec, sc)
        };
        // 空选区（刚按 V 就 Ctrl+A）高亮整段，避免“按了没反应”。
        let (cs, ce) = if lo == hi && cs == ce {
            let end = self.block_text(lo).map(|(_, len)| len).unwrap_or(ce);
            (cs, end.max(cs))
        } else {
            (cs, ce)
        };
        self.create_highlights(lo, cs, hi, ce);
        self.vim.mode = crate::ui::vim::VimMode::Normal;
        self.vim.labels = false;
    }

    fn undo_last_highlight(&mut self) {
        let Some(id) = self.last_highlight_undo.clone() else {
            return;
        };
        let Some(mut config) = self.book_config.clone() else {
            return;
        };
        config.highlights.retain(|h| h.id != id);
        config.notes.retain(|n| n.highlight_id != id);
        match config.save(&self.data_dir) {
            Ok(()) => {
                self.book_config = Some(config);
                if self
                    .note_popup
                    .as_ref()
                    .is_some_and(|p| p.highlight_id == id)
                {
                    self.note_popup = None;
                }
                self.last_highlight_undo = None;
            }
            Err(error) => self.error_msg = Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> (tempfile::TempDir, ReaderApp) {
        let directory = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(directory.path().to_string_lossy().into_owned());
        app.view = AppView::Reader;
        app.reader_introduction_seen = true;
        app.show_reader_introduction = false;
        app.reader_page_animation = "None".into();
        app.is_dual_column = false;
        app.scroll_mode = false;
        app.total_pages = 4;
        app.current_page = 1;
        (directory, app)
    }

    fn key(app: &mut ReaderApp, ctx: &egui::Context, key: egui::Key) {
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| app.handle_reader_shortcuts(ctx),
        );
    }

    #[test]
    fn page_arrows_dispatch_and_return_through_the_real_reader_actions() {
        let (_root, mut app) = app();
        let ctx = egui::Context::default();
        key(&mut app, &ctx, egui::Key::ArrowRight);
        assert_eq!(app.current_page, 2);
        key(&mut app, &ctx, egui::Key::ArrowLeft);
        assert_eq!(app.current_page, 1);
    }

    #[test]
    fn escape_closes_the_note_popup() {
        let (_root, mut app) = app();
        app.note_popup = Some(crate::ui::note_popup::NotePopup {
            highlight_id: "h".into(),
            buffer: String::new(),
            preview: "p".into(),
            focus: true,
            tag_buf: String::new(),
            tags: Vec::new(),
            book_id: None,
            tag_focus: false,
        });
        let ctx = egui::Context::default();
        key(&mut app, &ctx, egui::Key::Escape);
        assert!(app.note_popup.is_none());
    }
}
