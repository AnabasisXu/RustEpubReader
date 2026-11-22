//! The main reading interface, integrating text layout and UI overlays.
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use eframe::egui;
use egui::{Color32, FontId, UiBuilder};

use crate::app::{ReaderApp, TextSelection};
use reader_core::epub::ContentBlock;
use reader_core::library::HighlightColor;

use super::{reader_block::*, reader_state::*};

type BlockHighlightRanges = HashMap<usize, Vec<(usize, usize, HighlightColor)>>;
type ChapterHighlightRanges = HashMap<usize, BlockHighlightRanges>;

#[cfg(test)]
pub(crate) fn paginate_blocks(
    blocks: &[ContentBlock],
    font_size: f32,
    title_font_scale: f32,
    line_spacing: f32,
    available_height: f32,
    max_width: f32,
    media: &[reader_core::epub::parser::SupplementalImage],
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut page_start = 0;
    let mut current_h = 0.;
    let mut first_page = true;
    let first_is_heading = matches!(blocks.first(), Some(ContentBlock::Heading { .. }));
    let title_height = if first_is_heading {
        0.0
    } else {
        font_size * title_font_scale * 1.2 + TITLE_SPACING
    };
    let usable = (available_height - FRAME_MARGIN).max(100.);
    for (i, block) in blocks.iter().enumerate() {
        let bh = estimate_block_height(
            block,
            font_size,
            title_font_scale,
            font_size * line_spacing,
            max_width,
        );
        let images: Vec<_> = media
            .iter()
            .filter(|image| image.block == i)
            .cloned()
            .collect();
        let bh = super::reader_images::block_media_height(
            block,
            &images,
            bh,
            max_width,
            super::reader_images::page_limit_for(
                blocks,
                available_height,
                max_width,
                font_size,
                title_font_scale,
            ),
            font_size,
        );
        let budget = if first_page {
            usable - title_height
        } else {
            usable
        };
        if current_h + bh > budget && i > page_start {
            ranges.push((page_start, i));
            page_start = i;
            current_h = 0.;
            first_page = false;
        }
        current_h += bh;
    }
    if page_start < blocks.len() {
        ranges.push((page_start, blocks.len()));
    }
    ranges
}

impl ReaderApp {
    pub fn recalculate_pages(&mut self, ui: &mut egui::Ui, available_height: f32, max_width: f32) {
        set_spacing(
            self.line_spacing,
            self.para_spacing,
            self.text_indent as f32,
        );
        self.page_block_ranges =
            self.measured_chapter_pages(ui, self.current_chapter, available_height, max_width);
        self.total_pages = self.page_block_ranges.len().max(1);
        if let Some(block) = self.pending_restore_block.take() {
            self.current_page = self
                .page_block_ranges
                .iter()
                .enumerate()
                .filter(|(page, (start, end))| {
                    *start <= block
                        && block < *end
                        && (*start != block
                            || self
                                .page_block_ranges
                                .fragment(*page)
                                .is_none_or(|f| f.char_offset <= self.current_char_offset))
                })
                .map(|(page, _)| page)
                .next_back()
                .unwrap_or_else(|| self.total_pages.saturating_sub(1));
        }
        if self.current_page >= self.total_pages {
            self.current_page = self.total_pages.saturating_sub(1);
        }
        self.pages_dirty = false;
    }

    pub(crate) fn visible_page_contains_block(&self, block: usize) -> bool {
        let count = if self.is_dual_column { 2 } else { 1 };
        self.page_block_ranges
            .iter()
            .skip(self.current_page)
            .take(count)
            .any(|&(start, end)| start <= block && block < end)
    }

    pub fn render_reader(&mut self, ui: &mut egui::Ui) {
        // 卷页/静态页缓存里是旧画面；vim 叠加层一变化就丢弃缓存重绘。
        let mut vim_hash = std::collections::hash_map::DefaultHasher::new();
        (self.vim.mode != crate::ui::vim::VimMode::Off).hash(&mut vim_hash);
        (self.vim.mode == crate::ui::vim::VimMode::Visual).hash(&mut vim_hash);
        self.vim.labels.hash(&mut vim_hash);
        self.vim.cursor.hash(&mut vim_hash);
        self.vim.start.hash(&mut vim_hash);
        let vim_paint_key = vim_hash.finish();
        if vim_paint_key != self.vim_paint_key {
            self.vim_paint_key = vim_paint_key;
            self.page_curl.clear();
        }
        if self.native_book_path != self.book_path {
            self.reader_render_blocks = None;
            self.native_events.clear();
            self.publication_note = None;
            self.native_applied.clear();
            self.native_book_path = self.book_path.clone();
        }
        if let Some(book) = self.book.as_mut() {
            let mut start = self.current_chapter.saturating_sub(1);
            let mut end = if self.scroll_mode {
                (self.current_chapter + 4).min(book.chapters.len())
            } else {
                (self.current_chapter + 2).min(book.chapters.len())
            };
            if self.scroll_mode
                && !self
                    .continuous_scroll
                    .needs_reset(self.current_chapter, book.chapters.len())
            {
                start = start.min(self.continuous_scroll.start_chapter);
                end = end.max(self.continuous_scroll.loaded_end);
            }
            match book.ensure_chapter_window(start, end) {
                Ok(evicted) => {
                    for index in evicted {
                        self.native_applied.remove(&index);
                    }
                }
                Err(error) => {
                    self.error_msg = Some(error);
                    return;
                }
            }
            for index in start..end {
                let events = self.native_events.entry(index).or_default();
                let signature = (self.css_policy, self.scripts_enabled, events.len());
                if self.native_applied.get(&index) != Some(&signature) {
                    let previous: Vec<_> = book.chapters[index]
                        .blocks
                        .iter()
                        .map(ContentBlock::native_style)
                        .collect();
                    let errors = book.chapters[index].apply_presentation(
                        self.css_policy,
                        self.scripts_enabled,
                        events,
                    );
                    if !errors.is_empty() {
                        eprintln!(
                            "Publication script disabled for chapter {index}: {}",
                            errors.join("; ")
                        );
                    }
                    self.reader_render_blocks = None;
                    self.native_applied.insert(index, signature);
                    let changed = book.chapters[index]
                        .blocks
                        .iter()
                        .map(ContentBlock::native_style)
                        .ne(previous);
                    self.pages_dirty |=
                        changed && (self.scroll_mode || index == self.current_chapter);
                }
            }
        }
        // Push current typography settings into thread-locals
        set_spacing(
            self.line_spacing,
            self.para_spacing,
            self.text_indent as f32,
        );

        // vim 标签/光标只能定位到本帧绘制过的块；缓存本帧清空前，
        // 先抓上一帧的块位置（同一页重复渲染，滞后一帧无感）。
        let mut prev_blocks: Vec<(usize, f32)> = BLOCK_GALLEYS.with(|g| {
            g.borrow()
                .iter()
                .filter(|e| e.key.chapter == self.current_chapter)
                .map(|e| (e.key.block, e.rect.top()))
                .collect()
        });
        prev_blocks.sort_by(|a, b| a.1.total_cmp(&b.1));

        // Clear per-frame block galley cache
        BLOCK_GALLEYS.with(|bg| bg.borrow_mut().clear());

        if self.reader_page_animation == "Realistic" {
            self.page_anim_progress = 1.;
            self.page_anim_cross_chapter = false;
            self.page_anim_cross_chapter_snapshot = None;
        }
        if self.page_anim_progress < 1.0 {
            self.page_anim_progress =
                (self.page_anim_progress + self.reader_page_animation_speed).min(1.0);
            // Request repaint after a short delay to cap animation frame rate (~60fps)
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        if self.page_anim_progress >= 1.0 && self.page_anim_cross_chapter {
            self.page_anim_cross_chapter = false;
            self.page_anim_cross_chapter_snapshot = None;
        }

        let effective_font_family = if self.defer_custom_font_for_frame {
            "Sans".to_string()
        } else {
            "ReaderFont".to_string()
        };

        let full_rect = ui.available_rect_before_wrap();
        super::reader_images::configure(
            self.book.as_ref(),
            full_rect.height(),
            self.scroll_mode,
            self.font_size,
            self.title_font_scale,
        );
        self.paging_page_rect = Some(full_rect);
        if let Some(tex) = &self.reader_bg_texture {
            ui.painter().image(
                tex.id(),
                full_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::from_white_alpha((self.reader_bg_image_alpha * 255.0) as u8),
            );
        }

        let mut action_prev_page = false;
        let mut action_next_page = false;
        let mut clicked_link: Option<String> = None;
        let mut curl_consumed_frame = false;

        let current_chapter_data = self.book.as_ref().and_then(|book| {
            book.chapters.get(self.current_chapter).map(|chapter| {
                (
                    chapter.title.clone(),
                    {
                        if self.pages_dirty
                            || self
                                .reader_render_blocks
                                .as_ref()
                                .is_none_or(|(index, _)| *index != self.current_chapter)
                        {
                            self.reader_render_blocks =
                                Some((self.current_chapter, Arc::new(chapter.blocks.clone())));
                        }
                        self.reader_render_blocks.as_ref().unwrap().1.clone()
                    },
                    book.chapters.len(),
                )
            })
        });
        if let Some((title, blocks, total_ch)) = current_chapter_data {
            if self.reader_layout_anchor.is_some_and(|(chapter, block)| {
                chapter != self.current_chapter || !self.visible_page_contains_block(block)
            }) {
                self.reader_layout_anchor = None;
            }
            let available_width = ui.available_width();
            let available_height = ui.available_height();
            if (self.last_avail_width - available_width).abs() > 1.0
                || (self.last_avail_height - available_height).abs() > 1.0
            {
                if self.reader_layout_anchor.is_none()
                    && self.last_avail_width > 0.
                    && !self.pages_dirty
                {
                    self.reader_layout_anchor = Some((self.current_chapter, self.current_block));
                }
                if self.pending_restore_block.is_none() && !self.pages_dirty {
                    self.pending_restore_block = Some(
                        self.reader_layout_anchor
                            .or(self.panel_reflow_anchor)
                            .filter(|(chapter, _)| *chapter == self.current_chapter)
                            .map_or(self.current_block, |(_, block)| block),
                    );
                }
                self.pages_dirty = true;
                self.last_avail_width = available_width;
                self.last_avail_height = available_height;
            }
            let layout = reader_text_layout(
                available_width,
                available_height,
                self.scroll_mode,
                self.force_single_column,
            );
            let dual_column = layout.is_dual_column;

            self.is_dual_column = dual_column;
            let text_width = layout.text_width;
            let h_margin = layout.h_margin;
            if !self.scroll_mode && self.pages_dirty {
                if self.page_curl.source()
                    == Some(super::page_curl::PagePosition {
                        chapter: self.current_chapter,
                        page: self.current_page,
                    })
                    && self.pending_restore_block.is_none()
                {
                    self.pending_restore_block = Some(self.current_block);
                }
                self.page_curl.clear();
                self.recalculate_pages(ui, ui.available_height(), text_width);
            }
            if !self.scroll_mode && self.total_pages > 0 && self.current_page >= self.total_pages {
                self.current_page = self.total_pages - 1;
            }
            if dual_column && !self.current_page.is_multiple_of(2) {
                self.current_page = self.current_page.saturating_sub(1);
            }
            if !self.scroll_mode {
                if let Some(book) = &self.book {
                    super::reader_images::prefetch(
                        ui.ctx(),
                        book,
                        self.current_chapter,
                        &self.page_block_ranges,
                        self.current_page,
                        text_width,
                        self.font_size,
                    );
                }
            }
            let (block_start, block_end) = if self.scroll_mode {
                (0, blocks.len())
            } else if let Some(&(s, e)) = self.page_block_ranges.get(self.current_page) {
                (s.min(blocks.len()), e.min(blocks.len()))
            } else {
                (0, blocks.len())
            };
            let show_title = self.scroll_mode || self.current_page == 0;

            // Build per-block highlight ranges for the current chapter
            // Each block can have multiple highlight ranges with different colors
            let highlight_ranges: std::collections::HashMap<
                usize,
                Vec<(usize, usize, reader_core::library::HighlightColor)>,
            > = self
                .book_config
                .as_ref()
                .map(|cfg| {
                    let mut map: std::collections::HashMap<
                        usize,
                        Vec<(usize, usize, reader_core::library::HighlightColor)>,
                    > = std::collections::HashMap::new();
                    for h in cfg
                        .highlights
                        .iter()
                        .filter(|h| h.chapter == self.current_chapter)
                    {
                        // Only single-block highlights supported for now
                        map.entry(h.start_block).or_default().push((
                            h.start_offset,
                            h.end_offset,
                            h.color.clone(),
                        ));
                    }
                    map
                })
                .unwrap_or_default();

            {
                let mut vim_labels: std::collections::HashMap<usize, String> =
                    std::collections::HashMap::new();
                let mut vim_cursor: Option<(usize, usize)> = None;
                // 分页/翻页（含 Realistic 卷页）不会留下可复用的 galley 缓存，
                // 直接用当前页的块范围；连续滚动则用上一帧绘制过的块。
                let mut visible_blocks: Vec<usize> = if !self.vim.active() {
                    Vec::new()
                } else if self.scroll_mode {
                    prev_blocks.iter().map(|(block, _)| *block).collect()
                } else {
                    (block_start..block_end).collect()
                };
                visible_blocks.retain(|&block| {
                    matches!(
                        blocks.get(block),
                        Some(ContentBlock::Heading { .. } | ContentBlock::Paragraph { .. })
                    )
                });
                visible_blocks.dedup();
                if self.vim.active() {
                    if !self.vim.placed {
                        if let Some(&mid) = visible_blocks.get(visible_blocks.len() / 2) {
                            self.vim.cursor = (mid, 0);
                            self.vim.placed = true;
                        }
                    }
                    if self.vim.labels {
                        let keys = ['a','s','d','f','z','x','c','q','w','e','r','t','g','b','y','u','i','o','p','n'];
                        for (i, block) in visible_blocks.iter().take(keys.len()).enumerate() {
                            vim_labels.insert(*block, keys[i].to_string());
                        }
                    }
                    vim_cursor = Some(self.vim.cursor);
                }
                super::reader_block::set_vim_overlay(vim_labels, vim_cursor);
            }

            {
                let mut notes_map: std::collections::HashMap<(usize, usize), String> =
                    std::collections::HashMap::new();
                if let Some(cfg) = &self.book_config {
                    for h in &cfg.highlights {
                        if let Some(n) = cfg.notes.iter().find(|n| n.highlight_id == h.id) {
                            if !n.content.trim().is_empty() {
                                notes_map.insert((h.chapter, h.start_block), n.content.clone());
                            }
                        }
                    }
                }
                super::reader_block::set_highlight_notes(notes_map);
            }

            if self.scroll_mode {
                let mut layout_hasher = std::collections::hash_map::DefaultHasher::new();
                text_width.to_bits().hash(&mut layout_hasher);
                self.font_size.to_bits().hash(&mut layout_hasher);
                self.line_spacing.to_bits().hash(&mut layout_hasher);
                self.para_spacing.to_bits().hash(&mut layout_hasher);
                self.text_indent.hash(&mut layout_hasher);
                self.reader_font_family.hash(&mut layout_hasher);
                self.reader_cjk_font_family.hash(&mut layout_hasher);
                if self
                    .continuous_scroll
                    .update_layout_signature(layout_hasher.finish())
                {
                    self.scroll_to_top = true;
                }
                if self
                    .continuous_scroll
                    .needs_reset(self.current_chapter, total_ch)
                {
                    self.continuous_scroll.reset(self.current_chapter, total_ch);
                }
                let loaded_end = self.continuous_scroll.loaded_end;
                let start_chapter = self.continuous_scroll.start_chapter;
                let scroll_adjustment = self.continuous_scroll.take_scroll_adjustment();
                let continuous_chapters: Vec<(String, Vec<ContentBlock>)> = self
                    .book
                    .as_ref()
                    .map(|book| {
                        book.chapters[start_chapter..loaded_end]
                            .iter()
                            .map(|chapter| (chapter.title.clone(), chapter.blocks.clone()))
                            .collect()
                    })
                    .unwrap_or_default();
                let loaded_highlights: ChapterHighlightRanges = self
                    .book_config
                    .as_ref()
                    .map(|cfg| {
                        let mut by_chapter = ChapterHighlightRanges::new();
                        for highlight in cfg.highlights.iter().filter(|highlight| {
                            highlight.chapter >= start_chapter && highlight.chapter < loaded_end
                        }) {
                            by_chapter
                                .entry(highlight.chapter)
                                .or_default()
                                .entry(highlight.start_block)
                                .or_default()
                                .push((
                                    highlight.start_offset,
                                    highlight.end_offset,
                                    highlight.color.clone(),
                                ));
                        }
                        by_chapter
                    })
                    .unwrap_or_default();
                let empty_highlights: HashMap<
                    usize,
                    Vec<(usize, usize, reader_core::library::HighlightColor)>,
                > = HashMap::new();
                if ui.rect_contains_pointer(ui.available_rect_before_wrap())
                    && ui.input(|i| i.raw_scroll_delta.y != 0.0)
                {
                    let scroll_delta_y = ui.input(|i| i.raw_scroll_delta.y);
                    self.continuous_scroll
                        .allow_prepend_after_user_scroll(scroll_delta_y);
                }
                let programmed_scroll = self.scroll_to_top
                    || scroll_adjustment != 0.
                    || self.pending_restore_block.is_some();
                let mut scroll_area = egui::ScrollArea::vertical().auto_shrink([false; 2]);
                if self.scroll_to_top {
                    scroll_area = scroll_area.vertical_scroll_offset(0.0);
                    self.scroll_to_top = false;
                } else if scroll_adjustment != 0.0 {
                    scroll_area = scroll_area.vertical_scroll_offset(
                        (self.continuous_scroll.scroll_offset + scroll_adjustment).max(0.0),
                    );
                }
                let user_scrolling = ui.rect_contains_pointer(ui.available_rect_before_wrap())
                    && ui.input(|i| i.smooth_scroll_delta.y != 0. || i.pointer.primary_down());
                let scroll_output = scroll_area.show(ui, |ui| {
                    for (offset, (chapter_title, chapter_blocks)) in
                        continuous_chapters.iter().enumerate()
                    {
                        let chapter_idx = start_chapter + offset;
                        let chapter_top = ui.cursor().top();
                        let chapter_ranges = loaded_highlights.get(&chapter_idx).unwrap_or(
                            if chapter_idx == self.current_chapter {
                                &highlight_ranges
                            } else {
                                &empty_highlights
                            },
                        );
                        render_content_layout(
                            ui,
                            h_margin,
                            text_width,
                            chapter_title,
                            chapter_blocks,
                            0,
                            chapter_blocks.len(),
                            true,
                            None,
                            self.font_size,
                            self.title_font_scale,
                            self.reader_bg_color,
                            chapter_idx,
                            self.reader_font_color,
                            &effective_font_family,
                            &self.i18n,
                            &mut clicked_link,
                            chapter_ranges,
                        );
                        let chapter_height = (ui.cursor().top() - chapter_top).max(0.0);
                        self.continuous_scroll
                            .record_height(chapter_idx, chapter_height);
                    }
                    if let Some(target) = self.pending_restore_block {
                        BLOCK_GALLEYS.with(|galleys| {
                            if let Some(entry) = galleys.borrow().iter().find(|entry| {
                                entry.key.chapter == self.current_chapter
                                    && entry.key.block == target
                            }) {
                                let cursor = entry.galley.from_ccursor(egui::text::CCursor::new(
                                    self.current_char_offset,
                                ));
                                let target = entry
                                    .galley
                                    .pos_from_cursor(&cursor)
                                    .translate(entry.rect.min.to_vec2());
                                ui.scroll_to_rect(target, Some(egui::Align::Min));
                                self.pending_restore_block = None;
                            }
                        });
                    }
                });
                let viewport = scroll_output.inner_rect;
                let scroll_moved =
                    (scroll_output.state.offset.y - self.continuous_scroll.scroll_offset).abs()
                        > 0.01;
                self.continuous_scroll.record_scroll_output(
                    scroll_output.state.offset.y,
                    scroll_output.content_size.y,
                    viewport.height(),
                );
                let visible_block = BLOCK_GALLEYS.with(|galleys| {
                    galleys
                        .borrow()
                        .iter()
                        .filter(|entry| entry.rect.intersects(viewport))
                        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
                        .map(|entry| {
                            let cursor = entry.galley.cursor_from_pos(egui::vec2(
                                0.,
                                (viewport.top() - entry.rect.top()).max(0.),
                            ));
                            (entry.key.chapter, entry.key.block, cursor.ccursor.index)
                        })
                });
                if let Some((chapter, block, offset)) = visible_block {
                    self.continuous_scroll.set_visible_chapter(chapter);
                    if user_scrolling && scroll_moved && !programmed_scroll {
                        self.schedule_position_save_at(chapter, block, offset);
                    }
                }
                let near_start = self.continuous_scroll.near_start();
                let near_end = self.continuous_scroll.near_end();
                if near_start {
                    if self.continuous_scroll.prepend_previous().is_some() {
                        self.continuous_scroll.trim_bottom();
                        ui.ctx().request_repaint();
                    }
                } else if near_end && self.continuous_scroll.append_next(total_ch).is_some() {
                    self.continuous_scroll.trim_window();
                    ui.ctx().request_repaint();
                }
            } else {
                let page_rect = ui.available_rect_before_wrap();
                self.paging_page_rect = Some(page_rect);
                if self.reader_page_animation == "Realistic" {
                    curl_consumed_frame = self.render_curl_reader(
                        ui,
                        page_rect,
                        dual_column,
                        text_width,
                        h_margin,
                        &effective_font_family,
                        &mut clicked_link,
                    );
                } else if dual_column {
                    let col_w = (page_rect.width() - DUAL_COLUMN_GAP) / 2.0;
                    let left_rect = egui::Rect::from_min_size(
                        page_rect.min,
                        egui::vec2(col_w, page_rect.height()),
                    );
                    let right_rect = egui::Rect::from_min_size(
                        egui::pos2(page_rect.min.x + col_w + DUAL_COLUMN_GAP, page_rect.min.y),
                        egui::vec2(col_w, page_rect.height()),
                    );
                    let right_page = self.current_page + 1;
                    let is_anim_dual = self.reader_page_animation != "None"
                        && self.page_anim_progress < 1.0
                        && (self.page_anim_from != self.page_anim_to
                            || self.page_anim_cross_chapter);
                    if is_anim_dual {
                        let t = self.page_anim_progress;
                        let w = page_rect.width();
                        let dir = self.page_anim_direction;
                        let to_offset = egui::vec2(dir * (1.0 - t) * w, 0.0);
                        // "from" spread (sliding out, or static for Cover)
                        {
                            let from_offset = if self.reader_page_animation == "Cover" {
                                egui::vec2(0.0, 0.0)
                            } else {
                                egui::vec2(-dir * t * w, 0.0)
                            };
                            if let Some(snap) = &self.page_anim_cross_chapter_snapshot {
                                let snap_blocks = Arc::clone(&snap.blocks);
                                let snap_ranges = snap.block_ranges.clone();
                                let snap_total = snap.total_pages;
                                let snap_from = snap.from_page;
                                let snap_title = snap.title.clone();
                                let snap_chapter = snap.chapter;
                                let from_raw = snap_from.min(snap_total.saturating_sub(1));
                                let from_left = (from_raw / 2) * 2;
                                let (fls, fle) = snap_ranges
                                    .get(from_left)
                                    .copied()
                                    .map(|(s, e)| {
                                        (s.min(snap_blocks.len()), e.min(snap_blocks.len()))
                                    })
                                    .unwrap_or((0, snap_blocks.len()));
                                let left_from_rect = left_rect.translate(from_offset);
                                ui.allocate_new_ui(
                                    UiBuilder::new().max_rect(left_from_rect),
                                    |ui| {
                                        let clip = left_from_rect.intersect(page_rect);
                                        ui.set_clip_rect(clip);
                                        ui.painter().rect_filled(clip, 0.0, self.reader_bg_color);
                                        render_content_layout(
                                            ui,
                                            h_margin,
                                            text_width,
                                            &snap_title,
                                            &snap_blocks,
                                            fls,
                                            fle,
                                            from_left == 0,
                                            snap_ranges.fragment(from_left),
                                            self.font_size,
                                            self.title_font_scale,
                                            self.reader_bg_color,
                                            self.current_chapter,
                                            self.reader_font_color,
                                            &effective_font_family,
                                            &self.i18n,
                                            &mut clicked_link,
                                            &highlight_ranges,
                                        );
                                    },
                                );
                                let from_right = from_left + 1;
                                if from_right < snap_total {
                                    let (frs, fre) = snap_ranges
                                        .get(from_right)
                                        .copied()
                                        .map(|(s, e)| {
                                            (s.min(snap_blocks.len()), e.min(snap_blocks.len()))
                                        })
                                        .unwrap_or((0, 0));
                                    let right_from_rect = right_rect.translate(from_offset);
                                    ui.allocate_new_ui(
                                        UiBuilder::new().max_rect(right_from_rect),
                                        |ui| {
                                            let clip = right_from_rect.intersect(page_rect);
                                            ui.set_clip_rect(clip);
                                            ui.painter().rect_filled(
                                                clip,
                                                0.0,
                                                self.reader_bg_color,
                                            );
                                            render_content_layout(
                                                ui,
                                                h_margin,
                                                text_width,
                                                &snap_title,
                                                &snap_blocks,
                                                frs,
                                                fre,
                                                false,
                                                snap_ranges.fragment(from_right),
                                                self.font_size,
                                                self.title_font_scale,
                                                self.reader_bg_color,
                                                snap_chapter,
                                                self.reader_font_color,
                                                &effective_font_family,
                                                &self.i18n,
                                                &mut clicked_link,
                                                &highlight_ranges,
                                            );
                                        },
                                    );
                                }
                            } else {
                                let from_raw =
                                    self.page_anim_from.min(self.total_pages.saturating_sub(1));
                                let from_left = (from_raw / 2) * 2;
                                let (fls, fle) = self
                                    .page_block_ranges
                                    .get(from_left)
                                    .copied()
                                    .map(|(s, e)| (s.min(blocks.len()), e.min(blocks.len())))
                                    .unwrap_or((0, blocks.len()));
                                let left_from_rect = left_rect.translate(from_offset);
                                ui.allocate_new_ui(
                                    UiBuilder::new().max_rect(left_from_rect),
                                    |ui| {
                                        let clip = left_from_rect.intersect(page_rect);
                                        ui.set_clip_rect(clip);
                                        ui.painter().rect_filled(clip, 0.0, self.reader_bg_color);
                                        render_content_layout(
                                            ui,
                                            h_margin,
                                            text_width,
                                            &title,
                                            &blocks,
                                            fls,
                                            fle,
                                            from_left == 0,
                                            self.page_block_ranges.fragment(from_left),
                                            self.font_size,
                                            self.title_font_scale,
                                            self.reader_bg_color,
                                            self.current_chapter,
                                            self.reader_font_color,
                                            &effective_font_family,
                                            &self.i18n,
                                            &mut clicked_link,
                                            &highlight_ranges,
                                        );
                                    },
                                );
                                let from_right = from_left + 1;
                                if from_right < self.total_pages {
                                    let (frs, fre) = self
                                        .page_block_ranges
                                        .get(from_right)
                                        .copied()
                                        .map(|(s, e)| (s.min(blocks.len()), e.min(blocks.len())))
                                        .unwrap_or((0, 0));
                                    let right_from_rect = right_rect.translate(from_offset);
                                    ui.allocate_new_ui(
                                        UiBuilder::new().max_rect(right_from_rect),
                                        |ui| {
                                            let clip = right_from_rect.intersect(page_rect);
                                            ui.set_clip_rect(clip);
                                            ui.painter().rect_filled(
                                                clip,
                                                0.0,
                                                self.reader_bg_color,
                                            );
                                            render_content_layout(
                                                ui,
                                                h_margin,
                                                text_width,
                                                &title,
                                                &blocks,
                                                frs,
                                                fre,
                                                false,
                                                self.page_block_ranges.fragment(from_right),
                                                self.font_size,
                                                self.title_font_scale,
                                                self.reader_bg_color,
                                                self.current_chapter,
                                                self.reader_font_color,
                                                &effective_font_family,
                                                &self.i18n,
                                                &mut clicked_link,
                                                &highlight_ranges,
                                            );
                                        },
                                    );
                                }
                            }

                            // Cover animation: shadow on leading edge of incoming spread
                            if self.reader_page_animation == "Cover" {
                                let to_rect_pos = left_rect.translate(to_offset);
                                let shadow_w = 28.0f32;
                                let steps = 8u32;
                                for i in 0..steps {
                                    let sub_w = shadow_w / steps as f32;
                                    let (sub_x, alpha_val) = if dir > 0.0 {
                                        let x = to_rect_pos.left() - shadow_w + i as f32 * sub_w;
                                        let a = ((i + 1) as f32 * 70.0 / steps as f32) as u8;
                                        (x, a)
                                    } else {
                                        let x = to_rect_pos.right()
                                            + (page_rect.width() - left_rect.width())
                                            + i as f32 * sub_w;
                                        let a = ((steps - i) as f32 * 70.0 / steps as f32) as u8;
                                        (x, a)
                                    };
                                    let sub_rect = egui::Rect::from_min_size(
                                        egui::pos2(sub_x, page_rect.top()),
                                        egui::vec2(sub_w, page_rect.height()),
                                    );
                                    ui.painter().rect_filled(
                                        sub_rect,
                                        0.0,
                                        Color32::from_black_alpha(alpha_val),
                                    );
                                }
                            }
                        }
                        // "to" spread (sliding in)
                        let to_raw = self.page_anim_to.min(self.total_pages.saturating_sub(1));
                        let to_left = (to_raw / 2) * 2;
                        let (tls, tle) = self
                            .page_block_ranges
                            .get(to_left)
                            .copied()
                            .map(|(s, e)| (s.min(blocks.len()), e.min(blocks.len())))
                            .unwrap_or((0, blocks.len()));
                        let left_to_rect = left_rect.translate(to_offset);
                        ui.allocate_new_ui(UiBuilder::new().max_rect(left_to_rect), |ui| {
                            let clip = left_to_rect.intersect(page_rect);
                            ui.set_clip_rect(clip);
                            ui.painter().rect_filled(clip, 0.0, self.reader_bg_color);
                            render_content_layout(
                                ui,
                                h_margin,
                                text_width,
                                &title,
                                &blocks,
                                tls,
                                tle,
                                to_left == 0,
                                self.page_block_ranges.fragment(to_left),
                                self.font_size,
                                self.title_font_scale,
                                self.reader_bg_color,
                                self.current_chapter,
                                self.reader_font_color,
                                &effective_font_family,
                                &self.i18n,
                                &mut clicked_link,
                                &highlight_ranges,
                            );
                        });
                        let to_right = to_left + 1;
                        if to_right < self.total_pages {
                            let (trs, tre) = self
                                .page_block_ranges
                                .get(to_right)
                                .copied()
                                .map(|(s, e)| (s.min(blocks.len()), e.min(blocks.len())))
                                .unwrap_or((0, 0));
                            let right_to_rect = right_rect.translate(to_offset);
                            ui.allocate_new_ui(UiBuilder::new().max_rect(right_to_rect), |ui| {
                                let clip = right_to_rect.intersect(page_rect);
                                ui.set_clip_rect(clip);
                                ui.painter().rect_filled(clip, 0.0, self.reader_bg_color);
                                render_content_layout(
                                    ui,
                                    h_margin,
                                    text_width,
                                    &title,
                                    &blocks,
                                    trs,
                                    tre,
                                    false,
                                    self.page_block_ranges.fragment(to_right),
                                    self.font_size,
                                    self.title_font_scale,
                                    self.reader_bg_color,
                                    self.current_chapter,
                                    self.reader_font_color,
                                    &effective_font_family,
                                    &self.i18n,
                                    &mut clicked_link,
                                    &highlight_ranges,
                                );
                            });
                        }
                    } else {
                        ui.allocate_new_ui(UiBuilder::new().max_rect(left_rect), |ui| {
                            render_content_layout(
                                ui,
                                h_margin,
                                text_width,
                                &title,
                                &blocks,
                                block_start,
                                block_end,
                                show_title,
                                if self.scroll_mode {
                                    None
                                } else {
                                    self.page_block_ranges.fragment(self.current_page)
                                },
                                self.font_size,
                                self.title_font_scale,
                                self.reader_bg_color,
                                self.current_chapter,
                                self.reader_font_color,
                                &effective_font_family,
                                &self.i18n,
                                &mut clicked_link,
                                &highlight_ranges,
                            );
                        });
                        if right_page < self.total_pages {
                            let (rs, re) =
                                if let Some(&(s, e)) = self.page_block_ranges.get(right_page) {
                                    (s.min(blocks.len()), e.min(blocks.len()))
                                } else {
                                    (0, 0)
                                };
                            ui.allocate_new_ui(UiBuilder::new().max_rect(right_rect), |ui| {
                                render_content_layout(
                                    ui,
                                    h_margin,
                                    text_width,
                                    &title,
                                    &blocks,
                                    rs,
                                    re,
                                    right_page == 0,
                                    self.page_block_ranges.fragment(right_page),
                                    self.font_size,
                                    self.title_font_scale,
                                    self.reader_bg_color,
                                    self.current_chapter,
                                    self.reader_font_color,
                                    &effective_font_family,
                                    &self.i18n,
                                    &mut clicked_link,
                                    &highlight_ranges,
                                );
                            });
                        }
                    }
                    if !is_anim_dual {
                        let sep_x = page_rect.min.x + col_w + DUAL_COLUMN_GAP / 2.0;
                        ui.painter().line_segment(
                            [
                                egui::pos2(sep_x, page_rect.top() + 20.0),
                                egui::pos2(sep_x, page_rect.bottom() - 20.0),
                            ],
                            egui::Stroke::new(1.0_f32, Color32::from_gray(80)),
                        );
                    }
                    let page_info = if right_page < self.total_pages {
                        format!(
                            "{}-{} / {}",
                            self.current_page + 1,
                            right_page + 1,
                            self.total_pages
                        )
                    } else {
                        format!("{} / {}", self.current_page + 1, self.total_pages)
                    };
                    ui.painter().text(
                        egui::pos2(page_rect.right() - 20.0, page_rect.top() + 8.0),
                        egui::Align2::RIGHT_TOP,
                        page_info,
                        FontId::proportional(13.0),
                        Color32::GRAY,
                    );
                } else {
                    let is_animating = self.reader_page_animation != "None"
                        && self.page_anim_progress < 1.0
                        && (self.page_anim_from != self.page_anim_to
                            || self.page_anim_cross_chapter);

                    if is_animating {
                        let t = self.page_anim_progress;
                        let w = page_rect.width();
                        let dir = self.page_anim_direction;
                        let to_offset = egui::vec2(dir * (1.0 - t) * w, 0.0);

                        let to_idx = self.page_anim_to.min(self.total_pages.saturating_sub(1));
                        let (ts, te) = self
                            .page_block_ranges
                            .get(to_idx)
                            .copied()
                            .unwrap_or((0, blocks.len()));

                        {
                            let from_offset = if self.reader_page_animation == "Cover" {
                                egui::vec2(0.0, 0.0)
                            } else {
                                egui::vec2(-dir * t * w, 0.0)
                            };
                            if let Some(snap) = &self.page_anim_cross_chapter_snapshot {
                                let snap_blocks = Arc::clone(&snap.blocks);
                                let snap_ranges = snap.block_ranges.clone();
                                let snap_total = snap.total_pages;
                                let snap_from = snap.from_page;
                                let snap_title = snap.title.clone();
                                let snap_chapter = snap.chapter;
                                let from_idx = snap_from.min(snap_total.saturating_sub(1));
                                let (fs, fe) = snap_ranges
                                    .get(from_idx)
                                    .copied()
                                    .unwrap_or((0, snap_blocks.len()));
                                let from_rect = page_rect.translate(from_offset);
                                ui.allocate_new_ui(UiBuilder::new().max_rect(from_rect), |ui| {
                                    let clip = from_rect.intersect(page_rect);
                                    ui.set_clip_rect(clip);
                                    ui.painter().rect_filled(clip, 0.0, self.reader_bg_color);
                                    render_content_layout(
                                        ui,
                                        h_margin,
                                        text_width,
                                        &snap_title,
                                        &snap_blocks,
                                        fs.min(snap_blocks.len()),
                                        fe.min(snap_blocks.len()),
                                        from_idx == 0,
                                        snap_ranges.fragment(from_idx),
                                        self.font_size,
                                        self.title_font_scale,
                                        self.reader_bg_color,
                                        snap_chapter,
                                        self.reader_font_color,
                                        &effective_font_family,
                                        &self.i18n,
                                        &mut clicked_link,
                                        &highlight_ranges,
                                    );
                                });
                            } else {
                                let from_idx =
                                    self.page_anim_from.min(self.total_pages.saturating_sub(1));
                                let (fs, fe) = self
                                    .page_block_ranges
                                    .get(from_idx)
                                    .copied()
                                    .unwrap_or((0, blocks.len()));
                                let from_rect = page_rect.translate(from_offset);
                                ui.allocate_new_ui(UiBuilder::new().max_rect(from_rect), |ui| {
                                    let clip = from_rect.intersect(page_rect);
                                    ui.set_clip_rect(clip);
                                    ui.painter().rect_filled(clip, 0.0, self.reader_bg_color);
                                    render_content_layout(
                                        ui,
                                        h_margin,
                                        text_width,
                                        &title,
                                        &blocks,
                                        fs.min(blocks.len()),
                                        fe.min(blocks.len()),
                                        from_idx == 0,
                                        self.page_block_ranges.fragment(from_idx),
                                        self.font_size,
                                        self.title_font_scale,
                                        self.reader_bg_color,
                                        self.current_chapter,
                                        self.reader_font_color,
                                        &effective_font_family,
                                        &self.i18n,
                                        &mut clicked_link,
                                        &highlight_ranges,
                                    );
                                });
                            }

                            // Cover animation: draw shadow on leading edge of incoming page
                            if self.reader_page_animation == "Cover" {
                                let to_rect_pos = page_rect.translate(to_offset);
                                let shadow_w = 28.0f32;
                                let steps = 8u32;
                                for i in 0..steps {
                                    let sub_w = shadow_w / steps as f32;
                                    let (sub_x, alpha_val) = if dir > 0.0 {
                                        let x = to_rect_pos.left() - shadow_w + i as f32 * sub_w;
                                        let a = ((i + 1) as f32 * 70.0 / steps as f32) as u8;
                                        (x, a)
                                    } else {
                                        let x = to_rect_pos.right() + i as f32 * sub_w;
                                        let a = ((steps - i) as f32 * 70.0 / steps as f32) as u8;
                                        (x, a)
                                    };
                                    let sub_rect = egui::Rect::from_min_size(
                                        egui::pos2(sub_x, page_rect.top()),
                                        egui::vec2(sub_w, page_rect.height()),
                                    );
                                    ui.painter().rect_filled(
                                        sub_rect,
                                        0.0,
                                        Color32::from_black_alpha(alpha_val),
                                    );
                                }
                            }
                        }

                        let to_rect = page_rect.translate(to_offset);

                        ui.allocate_new_ui(UiBuilder::new().max_rect(to_rect), |ui| {
                            let clip = to_rect.intersect(page_rect);
                            ui.set_clip_rect(clip);
                            ui.painter().rect_filled(clip, 0.0, self.reader_bg_color);
                            render_content_layout(
                                ui,
                                h_margin,
                                text_width,
                                &title,
                                &blocks,
                                ts.min(blocks.len()),
                                te.min(blocks.len()),
                                to_idx == 0,
                                self.page_block_ranges.fragment(to_idx),
                                self.font_size,
                                self.title_font_scale,
                                self.reader_bg_color,
                                self.current_chapter,
                                self.reader_font_color,
                                &effective_font_family,
                                &self.i18n,
                                &mut clicked_link,
                                &highlight_ranges,
                            );
                        });
                    } else {
                        render_content_layout(
                            ui,
                            h_margin,
                            text_width,
                            &title,
                            &blocks,
                            block_start,
                            block_end,
                            show_title,
                            if self.scroll_mode {
                                None
                            } else {
                                self.page_block_ranges.fragment(self.current_page)
                            },
                            self.font_size,
                            self.title_font_scale,
                            self.reader_bg_color,
                            self.current_chapter,
                            self.reader_font_color,
                            &effective_font_family,
                            &self.i18n,
                            &mut clicked_link,
                            &highlight_ranges,
                        );
                    }
                    ui.painter().text(
                        egui::pos2(page_rect.right() - 20.0, page_rect.top() + 8.0),
                        egui::Align2::RIGHT_TOP,
                        format!("{} / {}", self.current_page + 1, self.total_pages),
                        FontId::proportional(13.0),
                        Color32::GRAY,
                    );
                }
                if !curl_consumed_frame
                    && self.text_selection.is_none()
                    && self.clicked_highlight_id.is_none()
                    && !self.show_review_panel
                {
                    let hover_pos = ui.input(|i| i.pointer.hover_pos());
                    let pointer_in_page = hover_pos.is_some_and(|pos| {
                        page_rect.contains(pos)
                            && ui
                                .ctx()
                                .layer_id_at(pos)
                                .is_none_or(|layer| layer.order <= ui.layer_id().order)
                    });
                    if pointer_in_page {
                        let scroll = ui.input(|i| i.raw_scroll_delta.y);
                        if scroll < -30.0 {
                            action_next_page = true;
                        } else if scroll > 30.0 {
                            action_prev_page = true;
                        }
                    }
                    // Click-to-turn is handled in the selection release handler
                    // to avoid conflict with sel_press_origin
                    if pointer_in_page
                        && clicked_link.is_none()
                        && self.sel_press_origin.is_none()
                        && ui.input(|i| i.pointer.primary_clicked())
                    {
                        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                            if page_rect.contains(pos) {
                                if super::reader_chrome::center_click(page_rect, pos) {
                                    self.reader_chrome.toggle(ui.input(|i| i.time));
                                } else if pos.x < page_rect.center().x {
                                    action_prev_page = true;
                                } else {
                                    action_next_page = true;
                                }
                            }
                        }
                    }
                }
            }
        } else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(self.i18n.t("reader.select_book_hint"))
                        .size(24.0)
                        .color(Color32::from_gray(128)),
                );
            });
        }

        if let Some(note) = &self.publication_note {
            let mut open = true;
            egui::Window::new(self.i18n.t("reader.publication_note"))
                .id(egui::Id::new("publication_note"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width(420.)
                .show(ui.ctx(), |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(400.)
                        .show(ui, |ui| {
                            ui.label(&note.text);
                        });
                });
            if !open {
                self.publication_note = None;
            }
        }
        let had_clicked_link = clicked_link.is_some();
        if let Some(url) = clicked_link {
            let url = url.trim().to_string();
            let lowered = url.to_lowercase();
            if let Some(href) = url.strip_prefix("reader-note:") {
                self.publication_note = self
                    .book
                    .as_ref()
                    .and_then(|book| book.native_note(self.current_chapter, href));
            } else if let Some(event) = url.strip_prefix("reader-event:") {
                if let Some((chapter, node)) = event
                    .split_once(':')
                    .and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))
                {
                    let events = self.native_events.entry(chapter).or_default();
                    if self.scripts_enabled && events.len() < 128 {
                        events.push(reader_core::epub::native::NativeEvent {
                            node,
                            kind: "click".into(),
                        });
                        self.pending_restore_block = Some(self.current_block);
                        self.pages_dirty = true;
                        self.page_curl.clear();
                    }
                }
            } else if lowered.starts_with("http://")
                || lowered.starts_with("https://")
                || lowered.starts_with("mailto:")
                || lowered.starts_with("tel:")
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            } else if !url.starts_with('#') {
                let normalized = normalize_epub_href(&url);
                let target_idx = if !normalized.is_empty() {
                    self.book.as_ref().and_then(|book| {
                        book.chapters.iter().position(|ch| {
                            let Some(ref src) = ch.source_href else {
                                return false;
                            };
                            let src_norm = normalize_epub_href(src);
                            src_norm == normalized
                                || src_norm.ends_with(&format!("/{normalized}"))
                                || normalized.ends_with(&format!("/{src_norm}"))
                        })
                    })
                } else {
                    None
                };
                if let Some(idx) = target_idx {
                    // Check if target is a review chapter (娈佃瘎) 鈥?show overlay instead of navigating
                    if self
                        .book
                        .as_ref()
                        .is_some_and(|b| b.review_chapter_indices.contains(&idx))
                    {
                        self.show_review_panel = true;
                        self.review_panel_chapter = Some(idx);
                        self.review_panel_anchor = url.split('#').nth(1).map(|s| s.to_string());
                        self.review_panel_just_opened = true;
                    } else {
                        self.navigate_reader(
                            super::reader_navigation::ReaderLocation {
                                chapter: idx,
                                block: 0,
                                char_offset: 0,
                            },
                            super::reader_navigation::NavigationKind::Local,
                        );
                    }
                } else {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            }
        }
        if action_prev_page {
            self.turn_reader_page(false);
        }
        if action_next_page {
            self.turn_reader_page(true);
        }
        if !self.scroll_mode
            && !curl_consumed_frame
            && !self.page_curl.busy()
            && self.panel_reflow_anchor.is_none()
        {
            if let Some((block, _)) = self.page_block_ranges.get(self.current_page).copied() {
                let anchor = self.reader_layout_anchor.filter(|&(chapter, anchor)| {
                    chapter == self.current_chapter && self.visible_page_contains_block(anchor)
                });
                let offset = self
                    .page_block_ranges
                    .fragment(self.current_page)
                    .map_or(0, |f| f.char_offset);
                // A different viewport may put the same saved locator halfway down
                // a page. Layout/remote restoration must not become a fresh edit.
                let locator_page = self
                    .page_block_ranges
                    .iter()
                    .enumerate()
                    .filter(|(page, (start, end))| {
                        *start <= self.current_block
                            && self.current_block < *end
                            && (*start != self.current_block
                                || self
                                    .page_block_ranges
                                    .fragment(*page)
                                    .is_none_or(|f| f.char_offset <= self.current_char_offset))
                    })
                    .map(|(page, _)| page)
                    .next_back();
                let preserve = (self.last_synced_chapter == Some(self.current_chapter)
                    || self.reader_navigation.restored_locator == Some(self.reader_location()))
                    && locator_page.is_some_and(|page| {
                        page >= self.current_page
                            && page < self.current_page + if self.is_dual_column { 2 } else { 1 }
                    });
                if !preserve {
                    self.schedule_position_save_at(
                        self.current_chapter,
                        anchor.map_or(block, |(_, b)| b),
                        offset,
                    );
                }
            }
        }
        if curl_consumed_frame {
            return;
        }

        // 鈹€鈹€ Custom text selection state machine 鈹€鈹€
        let block_galleys: Vec<BlockGalleyEntry> =
            BLOCK_GALLEYS.with(|bg| bg.borrow_mut().drain(..).collect());

        // Detect primary pointer press / drag / release for selection
        let pointer_pos = ui.ctx().input(|i| i.pointer.interact_pos());
        let primary_down = ui.ctx().input(|i| i.pointer.primary_down());
        let primary_pressed = ui.ctx().input(|i| i.pointer.primary_pressed());
        let primary_released = ui.ctx().input(|i| i.pointer.primary_released());
        let press_pos = ui.ctx().input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            })
        });

        // Helper: find which block a screen position falls into and return (block_idx, char_offset)
        let hit_test = |pos: egui::Pos2| -> Option<(usize, usize)> {
            for entry in &block_galleys {
                if entry.key.chapter == self.current_chapter
                    && entry.rect.intersect(entry.clip).contains(pos)
                {
                    let local = egui::vec2(pos.x - entry.rect.min.x, pos.y - entry.rect.min.y);
                    let cursor = entry.galley.cursor_from_pos(local);
                    return Some((entry.key.block, cursor.ccursor.index));
                }
            }
            None
        };

        // Nearest text position when pressing in blank margins, so dragging from
        // the left edge of a line still starts a selection.
        let nearest_hit = |pos: egui::Pos2| -> Option<(usize, usize)> {
            let entry = block_galleys
                .iter()
                .filter(|entry| {
                    entry.key.chapter == self.current_chapter
                        && entry.rect.height() > 0.
                        && entry.rect.intersect(entry.clip).is_positive()
                })
                .min_by(|a, b| {
                    let da = (pos.y - a.rect.center().y).abs()
                        + (pos.x - a.rect.center().x).abs() * 0.01;
                    let db = (pos.y - b.rect.center().y).abs()
                        + (pos.x - b.rect.center().x).abs() * 0.01;
                    da.total_cmp(&db)
                })?;
            let x = pos.x.clamp(entry.rect.left() + 1., entry.rect.right() - 1.);
            let y = pos.y.clamp(entry.rect.top() + 1., entry.rect.bottom() - 1.);
            let cursor = entry
                .galley
                .cursor_from_pos(egui::vec2(x - entry.rect.left(), y - entry.rect.top()));
            Some((entry.key.block, cursor.ccursor.index))
        };

        // Check if pointer is over a toolbar area (so we don't start selection there)
        let toolbar_id = egui::Id::new("sel_toolbar");
        let note_toolbar_id = egui::Id::new("hl_note_toolbar");
        let over_toolbar = ui.ctx().memory(|mem| {
            mem.layer_id_at(pointer_pos.unwrap_or_default())
                .is_some_and(|layer| {
                    layer.id == toolbar_id
                        || layer.id == note_toolbar_id
                        || layer.order > ui.layer_id().order
                })
        });

        if let Some(pos) = pointer_pos {
            const DRAG_THRESHOLD: f32 = 5.0;
            if primary_pressed && !over_toolbar {
                let origin = press_pos.unwrap_or(pos);
                if let Some((block_idx, char_idx)) = hit_test(origin).or_else(|| nearest_hit(origin)) {
                    // Record press origin; don't create TextSelection yet
                    self.sel_press_origin = Some((origin, block_idx, char_idx));
                    // Clear any existing finalized selection or highlight popup
                    if self.text_selection.as_ref().is_some_and(|s| !s.is_dragging) {
                        self.text_selection = None;
                    }
                    if self.clicked_highlight_id.is_some() {
                        self.clicked_highlight_id = None;
                    }
                } else {
                    // Clicked outside any block 鈫?clear everything
                    self.sel_press_origin = None;
                    self.text_selection = None;
                    self.clicked_highlight_id = None;
                }
            }
            // Press, motion and release can arrive in one rendered frame.
            // Retain the event's press origin and finalize that drag as well.
            if (primary_down || primary_released) && !over_toolbar {
                // If we have a pending press origin but no selection yet, check threshold
                if let Some((origin, block_idx, char_idx)) = self.sel_press_origin {
                    if (pos - origin).length() >= DRAG_THRESHOLD {
                        // Threshold exceeded 鈫?promote to real selection
                        let cur_hit = hit_test(pos).unwrap_or((block_idx, char_idx));
                        self.text_selection = Some(TextSelection {
                            start_block: block_idx,
                            start_char: char_idx,
                            end_block: cur_hit.0,
                            end_char: cur_hit.1,
                            is_dragging: true,
                        });
                        self.sel_press_origin = None;
                    }
                }
                // Update end of an active selection while dragging
                if let Some(sel) = &mut self.text_selection {
                    if sel.is_dragging {
                        if let Some((block_idx, char_idx)) = hit_test(pos) {
                            sel.end_block = block_idx;
                            sel.end_char = char_idx;
                        } else {
                            // Pointer is outside any block 鈥?find the closest block
                            // above or below to extend selection
                            let mut best: Option<(usize, usize)> = None;
                            for entry in block_galleys
                                .iter()
                                .filter(|entry| entry.key.chapter == self.current_chapter)
                            {
                                if pos.y < entry.rect.min.y {
                                    // Above this block 鈫?first char
                                    if best
                                        .as_ref()
                                        .is_none_or(|(best_idx, _)| entry.key.block < *best_idx)
                                    {
                                        best = Some((entry.key.block, 0));
                                    }
                                    break;
                                } else if pos.y > entry.rect.max.y {
                                    // Below this block 鈫?last char
                                    let end = entry.galley.text().chars().count();
                                    best = Some((entry.key.block, end));
                                }
                            }
                            if let Some((bi, ci)) = best {
                                sel.end_block = bi;
                                sel.end_char = ci;
                            }
                        }
                    }
                }
            }

            if primary_released {
                // Check if this was a click (no drag) on a highlighted region
                let mut handled_as_highlight = false;
                if let Some((press_pos, press_block, press_char)) = self.sel_press_origin.take() {
                    // Look up if this block+char sits inside a highlight
                    if let Some(cfg) = &self.book_config {
                        if let Some(hl) = cfg.highlights.iter().find(|h| {
                            h.chapter == self.current_chapter
                                && h.start_block == press_block
                                && press_char >= h.start_offset
                                && press_char < h.end_offset
                        }) {
                            // Found a highlight under the click 鈫?show note popup
                            handled_as_highlight = true;
                            self.clicked_highlight_id = Some(hl.id.clone());
                            self.hl_note_just_opened = true;
                            // Load existing note content into edit buffer
                            if let Some(note) = cfg.notes.iter().find(|n| n.highlight_id == hl.id) {
                                self.editing_note_buf = note.content.clone();
                                self.editing_note_original = Some(note.content.clone());
                            } else {
                                self.editing_note_buf.clear();
                                self.editing_note_original = None;
                            }
                            // Position popup above the click point
                            if let Some(entry) = block_galleys.iter().find(|entry| {
                                entry.key.chapter == self.current_chapter
                                    && entry.key.block == press_block
                            }) {
                                self.hl_note_toolbar_pos =
                                    egui::pos2(entry.rect.center().x, entry.rect.min.y);
                            }
                            // Clear any text selection
                            self.text_selection = None;
                        }
                    }
                    // Plain click on text (no highlight, no drag) 鈫?page turn
                    if !had_clicked_link
                        && !handled_as_highlight
                        && !self.scroll_mode
                        && self.text_selection.is_none()
                        && !self.show_review_panel
                    {
                        if let Some(page_rect) = self.paging_page_rect {
                            if page_rect.contains(press_pos) {
                                if super::reader_chrome::center_click(page_rect, press_pos) {
                                    self.reader_chrome.toggle(ui.input(|i| i.time));
                                } else if self.mouse_page_turn {
                                    self.turn_reader_page(press_pos.x >= page_rect.center().x);
                                }
                            }
                        }
                    }
                }

                if let Some(sel) = &mut self.text_selection {
                    if sel.is_dragging {
                        sel.is_dragging = false;
                        // If selection is empty (start == end), clear it
                        if sel.start_block == sel.end_block && sel.start_char == sel.end_char {
                            self.text_selection = None;
                        } else {
                            // Position the toolbar above the start of the selection
                            let (sel_start_block, _) = sel.normalized();
                            if let Some(entry) = block_galleys.iter().find(|entry| {
                                entry.key.chapter == self.current_chapter
                                    && entry.key.block == sel_start_block
                            }) {
                                self.sel_toolbar_pos =
                                    egui::pos2(entry.rect.center().x, entry.rect.top());
                            }
                        }
                    }
                }
            }
        }

        // 鈹€鈹€ Draw selection highlight overlay (blue rectangles) 鈹€鈹€
        if let Some(sel) = &self.text_selection {
            let (sb, sc, eb, ec) = sel.normalized_range();
            for entry in block_galleys
                .iter()
                .filter(|entry| entry.key.chapter == self.current_chapter)
            {
                let idx = entry.key.block;
                if idx < sb || idx > eb {
                    continue;
                }
                let char_len = entry.text.chars().count();
                let sel_start = if idx == sb { sc } else { 0 };
                let sel_end = if idx == eb {
                    ec.min(char_len)
                } else {
                    char_len
                };
                if sel_start >= sel_end {
                    continue;
                }
                // Convert char offsets to galley cursors
                let c_start = entry
                    .galley
                    .from_ccursor(egui::text::CCursor::new(sel_start));
                let c_end = entry.galley.from_ccursor(egui::text::CCursor::new(sel_end));
                // Walk galley rows and draw highlight rect for each selected row range
                let start_row = c_start.rcursor.row;
                let end_row = c_end.rcursor.row;
                for row_idx in start_row..=end_row {
                    if row_idx >= entry.galley.rows.len() {
                        break;
                    }
                    let row = &entry.galley.rows[row_idx];
                    let row_min_x = if row_idx == start_row {
                        // Start of selection within first row

                        entry.galley.pos_from_cursor(&c_start).min.x
                    } else {
                        row.rect.min.x
                    };
                    let row_max_x = if row_idx == end_row {
                        entry.galley.pos_from_cursor(&c_end).max.x
                    } else {
                        row.rect.max.x
                    };
                    let hl_rect = egui::Rect::from_min_max(
                        egui::pos2(
                            entry.rect.min.x + row_min_x,
                            entry.rect.min.y + row.rect.min.y,
                        ),
                        egui::pos2(
                            entry.rect.min.x + row_max_x,
                            entry.rect.min.y + row.rect.max.y,
                        ),
                    );
                    ui.painter()
                        .with_clip_rect(entry.clip)
                        .rect_filled(hl_rect, 0.0, SEL_BG);
                }
            }
        }

        // 鈹€鈹€ Extract selected text from block galleys 鈹€鈹€
        let selected_text: String = self
            .text_selection
            .as_ref()
            .filter(|s| !s.is_dragging || primary_down)
            .map(|sel| {
                let (sb, sc, eb, ec) = sel.normalized_range();
                let mut result = String::new();
                for entry in block_galleys
                    .iter()
                    .filter(|entry| entry.key.chapter == self.current_chapter)
                {
                    let idx = entry.key.block;
                    if idx < sb || idx > eb {
                        continue;
                    }
                    let chars: Vec<char> = entry.text.chars().collect();
                    let start = if idx == sb { sc } else { 0 };
                    let end = if idx == eb {
                        ec.min(chars.len())
                    } else {
                        chars.len()
                    };
                    if start < end {
                        if !result.is_empty() {
                            result.push('\n');
                        }
                        result.extend(&chars[start..end]);
                    }
                }
                result
            })
            .unwrap_or_default();

        // 鈹€鈹€ Show floating selection toolbar (when selection finalized) 鈹€鈹€
        if let Some(sel) = &self.text_selection {
            if !sel.is_dragging && !selected_text.is_empty() {
                let (sb, _, eb, _) = sel.normalized_range();
                let has_hl = self.book_config.as_ref().is_some_and(|cfg| {
                    cfg.highlights.iter().any(|h| {
                        h.chapter == self.current_chapter
                            && h.start_block >= sb
                            && h.start_block <= eb
                    })
                });
                let res = show_selection_toolbar(
                    ui.ctx(),
                    &self.i18n,
                    &selected_text,
                    self.sel_toolbar_pos,
                    has_hl,
                );
                match res {
                    SelToolbarResult::KeepOpen => {}
                    SelToolbarResult::Close => {
                        self.text_selection = None;
                    }
                    SelToolbarResult::CreateHighlight(color) => {
                        let sel_range = sel.normalized_range();
                        let mut created_id: Option<String> = None;
                        let mut preview_text = String::new();
                        if let Some(cfg) = &mut self.book_config {
                            let (sb, sc, eb, ec) = sel_range;
                            for entry in block_galleys
                                .iter()
                                .filter(|entry| entry.key.chapter == self.current_chapter)
                            {
                                let idx = entry.key.block;
                                if idx < sb || idx > eb {
                                    continue;
                                }
                                let char_len = entry.text.chars().count();
                                let start = if idx == sb { sc } else { 0 };
                                let end = if idx == eb {
                                    ec.min(char_len)
                                } else {
                                    char_len
                                };
                                if start < end {
                                    let slice_text: String =
                                        entry.text.chars().skip(start).take(end - start).collect();
                                    preview_text.push_str(
                                        &entry.text.chars().skip(start).take(end - start).collect::<String>(),
                                    );
                                    let new_id = reader_core::annotations::new_highlight_id();
                                    created_id = Some(new_id.clone());
                                    cfg.highlights.push(reader_core::library::Highlight {
                                        id: new_id,
                                        chapter: self.current_chapter,
                                        start_block: idx,
                                        start_offset: start,
                                        end_block: idx,
                                        end_offset: end,
                                        color: color.clone(),
                                        created_at: reader_core::now_secs(),
                                        tags: Vec::new(),
                                        text: slice_text,
                                    });
                                }
                            }
                            if let Err(error) = cfg.save(&self.data_dir) {
                                self.error_msg = Some(error.to_string());
                            };
                        }
                        // 仿 BN：高亮完成后直接弹出备注输入框
                        if let Some(id) = created_id {
                            self.note_popup = Some(crate::ui::note_popup::NotePopup {
                                highlight_id: id.clone(),
                                buffer: String::new(),
                                focus: true,
                                preview: preview_text.chars().take(160).collect(),
                                tag_buf: String::new(),
                    book_id: None,
                    tag_focus: false,
                            });
                            self.last_highlight_undo = Some(id);
                        }
                        self.text_selection = None;
                    }
                    SelToolbarResult::DeleteHighlight => {
                        let (sb, _, eb, _) = sel.normalized_range();
                        if let Some(cfg) = &mut self.book_config {
                            cfg.highlights.retain(|h| {
                                !(h.chapter == self.current_chapter
                                    && h.start_block >= sb
                                    && h.start_block <= eb)
                            });
                            if let Err(error) = cfg.save(&self.data_dir) {
                                self.error_msg = Some(error.to_string());
                            };
                        }
                        self.text_selection = None;
                    }
                }
            }
        }

        // 鈹€鈹€ Floating note popup for clicked highlight 鈹€鈹€
        if let Some(hl_id) = self.clicked_highlight_id.clone() {
            let popup_pos = self.hl_note_toolbar_pos;

            let note_toolbar_id = egui::Id::new("hl_note_toolbar");
            let mut close_popup = false;

            egui::Area::new(note_toolbar_id)
                .fixed_pos(egui::pos2(popup_pos.x - 160.0, popup_pos.y - 170.0))
                .order(egui::Order::Foreground)
                .interactable(true)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(320.0);

                        // Top row: highlight info + action buttons
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(self.i18n.t("context.note"))
                                    .strong()
                                    .size(13.0),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Delete highlight button
                                    if ui
                                        .small_button("删除")
                                        .on_hover_text(self.i18n.t("context.delete_highlight"))
                                        .clicked()
                                    {
                                        if let Some(cfg) = &mut self.book_config {
                                            cfg.notes.retain(|n| n.highlight_id != hl_id);
                                            cfg.highlights.retain(|h| h.id != hl_id);
                                            if let Err(error) = cfg.save(&self.data_dir) {
                                                self.error_msg = Some(error.to_string());
                                            };
                                        }
                                        close_popup = true;
                                    }
                                    // Close button
                                    if ui.small_button("x").clicked() {
                                        close_popup = true;
                                    }
                                },
                            );
                        });

                        ui.separator();

                        // Note text edit area
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut self.editing_note_buf)
                                .margin(super::design::Geometry::CARD_INSET)
                                .desired_rows(3)
                                .desired_width(300.0)
                        );

                        // Save button
                        ui.horizontal(|ui| {
                            if ui.button(self.i18n.t("context.save_note")).clicked()
                                || (response.lost_focus()
                                    && ui.input(|i| {
                                        i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl
                                    }))
                            {
                                if let Some(id) = self.book_config.as_ref().map(|c| c.id.clone()) {
                                    match reader_core::annotations::save_note(
                                        &self.data_dir,
                                        &id,
                                        &hl_id,
                                        &self.editing_note_buf,
                                        Some(self.editing_note_original.as_deref()),
                                    ) {
                                        Ok(config) => {
                                            self.book_config = Some(config);
                                            close_popup = true;
                                        }
                                        Err(error) => self.error_msg = Some(error),
                                    }
                                }
                            }
                        });
                    });
                });

            // Close popup on click outside (skip the frame it was just opened)
            if !close_popup && !self.show_review_panel {
                if self.hl_note_just_opened {
                    self.hl_note_just_opened = false;
                } else {
                    let any_click = ui.ctx().input(|i| i.pointer.primary_clicked());
                    if any_click {
                        let over_note_popup = ui.ctx().memory(|mem| {
                            mem.layer_id_at(pointer_pos.unwrap_or_default())
                                .is_some_and(|layer| layer.id == note_toolbar_id)
                        });
                        if !over_note_popup {
                            close_popup = true;
                        }
                    }
                }
            }

            if close_popup {
                self.clicked_highlight_id = None;
                self.editing_note_buf.clear();
            }
        }


    }
}
