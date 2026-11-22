//! 「笔记」页：按书分组展示全部笔记，支持搜索、跳转、删除、标签、导出。
use crate::app::{AppView, ReaderApp};
use eframe::egui;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct NotesPage {
    pub query: String,
    pub focus: bool,
    pub rows: Vec<Row>,
    pub dirty: bool,
    pub tags: Vec<String>,
    pub selected: HashSet<String>,
    pub confirm: Option<(String, String)>, // book_id, highlight_id
    pub confirm_batch: Vec<(String, String)>,
    pub confirm_focus: bool,
    pub tag_target: Option<(String, String, String)>, // book_id, highlight_id, buffer
    pub untagged: bool,
    pub text_cache: HashMap<String, HashMap<(usize, usize), String>>,
    tag_loaded: bool,
}

#[derive(Clone)]
pub(crate) struct Row {
    pub id: String,
    pub book_id: String,
    pub book_title: String,
    pub path: String,
    pub chapter: usize,
    pub chapter_title: String,
    pub block: usize,
    pub text: String,
    pub note: String,
    pub tags: Vec<String>,
}

impl ReaderApp {
    pub(crate) fn render_notes_page(&mut self, ui: &mut egui::Ui) {
        self.notes_page_load_tags();
        if self.notes_page.dirty || self.notes_page.rows.is_empty() {
            self.notes_page_rebuild();
        }
        // 左侧标签树：全部 / 未标记 / 各标签（含计数），点击过滤
        let all_tags = self.notes_page.tags.clone();
        let tree_rows = self.notes_page.rows.clone();
        egui::SidePanel::left("notes_tag_tree")
            .resizable(false)
            .exact_width(176.)
            .show_inside(ui, |ui| {
                ui.add_space(6.);
                ui.strong("标签");
                ui.add_space(4.);
                let total = tree_rows.len();
                if ui
                    .selectable_label(
                        self.notes_page.query.is_empty() && !self.notes_page.untagged,
                        format!("全部 ({total})"),
                    )
                    .clicked()
                {
                    self.notes_page.query.clear();
                    self.notes_page.untagged = false;
                    self.notes_page.dirty = true;
                }
                let untagged = tree_rows.iter().filter(|r| r.tags.is_empty()).count();
                if ui
                    .selectable_label(self.notes_page.untagged, format!("未标记 ({untagged})"))
                    .clicked()
                {
                    self.notes_page.untagged = !self.notes_page.untagged;
                    self.notes_page.query.clear();
                    self.notes_page.dirty = true;
                }
                ui.separator();
                egui::ScrollArea::vertical().id_salt("notes_tag_tree_scroll").show(ui, |ui| {
                    for tag in &all_tags {
                        let count = tree_rows.iter().filter(|r| r.tags.iter().any(|t| t == tag)).count();
                        let on = self.notes_page.query.split_whitespace().any(|w| w == tag);
                        if ui.selectable_label(on, format!("#{tag} ({count})")).clicked() {
                            if on {
                                self.notes_page.query = self
                                    .notes_page
                                    .query
                                    .split_whitespace()
                                    .filter(|w| *w != tag)
                                    .collect::<Vec<_>>()
                                    .join(" ");
                            } else {
                                if !self.notes_page.query.is_empty() {
                                    self.notes_page.query.push(' ');
                                }
                                self.notes_page.query.push_str(tag);
                            }
                            self.notes_page.dirty = true;
                        }
                    }
                });
            });
        ui.add_space(6.);

        ui.add_space(14.);
        ui.horizontal(|ui| {
            ui.heading(self.i18n.t("notes.title"));
            ui.add_space(16.);
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.notes_page.query)
                    .desired_width(320.),
            );
            if self.notes_page.focus {
                resp.request_focus();
                self.notes_page.focus = false;
            }
            if resp.changed() {
                self.notes_page.dirty = true;
            }
        });
        ui.add_space(8.);

        let rows = self.notes_page.rows.clone();
        ui.horizontal(|ui| {
            if ui.button("全选").clicked() {
                for r in &rows {
                    self.notes_page.selected.insert(format!("{}:{}", r.book_id, r.id));
                }
            }
            if ui.button("清空").clicked() {
                self.notes_page.selected.clear();
            }
            ui.small(format!("已选 {}", self.notes_page.selected.len()));
            if ui.button("删除选中").clicked() {
                self.notes_page.confirm_focus = true;
                self.notes_page.confirm_batch = self
                    .notes_page
                    .selected
                    .iter()
                    .filter_map(|k| k.split_once(':').map(|(b, h)| (b.to_string(), h.to_string())))
                    .collect();
            }
        });
        if rows.is_empty() {
            ui.label("暂无笔记");
            return;
        }
        let mut jump: Option<Row> = None;
        let mut edit_note: Option<Row> = None;
        let mut delete: Option<(String, String)> = None;
        let mut tag_edit: Option<(String, String, String)> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut last_book = String::new();
            for row in rows {
                if row.book_title != last_book {
                    ui.add_space(10.);
                    ui.label(egui::RichText::new(&row.book_title).strong());
                    last_book = row.book_title.clone();
                }
                let key = format!("{}:{}", row.book_id, row.id);
                ui.horizontal(|ui| {
                    let mut checked = self.notes_page.selected.contains(&key);
                    if ui.checkbox(&mut checked, "").changed() {
                        if checked {
                            self.notes_page.selected.insert(key.clone());
                        } else {
                            self.notes_page.selected.remove(&key);
                        }
                    }
                    ui.vertical(|ui| {
                        if ui
                            .add(
                                egui::Label::new(
                                    egui::RichText::new(row.text.as_str())
                                        .color(egui::Color32::from_rgb(255, 200, 60)),
                                )
                                .truncate()
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text(&row.text)
                            .clicked()
                        {
                            jump = Some(row.clone());
                        }
                        if !row.note.is_empty() {
                            ui.label(
                                egui::RichText::new(row.note.as_str())
                                    .small()
                                    .color(egui::Color32::from_rgb(120, 180, 255)),
                            );
                        }
                        ui.horizontal(|ui| {
                            ui.small(format!("第 {} 章 · {}", row.chapter + 1, row.chapter_title));
                            for tag in &row.tags {
                                ui.small(format!("#{tag}"));
                            }
                            if ui.small_button("编辑").clicked() {
                                edit_note = Some(row.clone());
                            }
                            if ui.small_button("标签").clicked() {
                                tag_edit = Some((row.book_id.clone(), row.id.clone(), String::new()));
                            }
                            if ui.small_button("删除").clicked() {
                                delete = Some((row.book_id.clone(), row.id.clone()));
                            }
                        });
                    });
                });
                ui.separator();
            }
        });

        if let Some(row) = edit_note {
            self.note_popup = Some(crate::ui::note_popup::NotePopup {
                highlight_id: row.id.clone(),
                buffer: row.note.clone(),
                preview: row.text.clone(),
                focus: true,
                tag_buf: String::new(),
                tags: row.tags.clone(),
                book_id: Some(row.book_id.clone()),
                tag_focus: false,
            });
        }
        if let Some(row) = jump {
            if self.book_path.as_deref() == Some(row.path.as_str()) {
                self.navigate_reader(
                    super::reader_navigation::ReaderLocation {
                        chapter: row.chapter,
                        block: row.block,
                        char_offset: 0,
                    },
                    super::reader_navigation::NavigationKind::Local,
                );
                self.view = AppView::Reader;
            } else {
                self.open_book_from_path(&row.path, Some(row.chapter));
                self.pending_restore_block = Some(row.block);
            }
        }
        if let Some((book_id, block)) = delete {
            self.notes_page.confirm = Some((book_id, block));
        }
        if let Some(t) = tag_edit {
            self.notes_page.tag_target = Some(t);
        }
        self.notes_page_modals(ctx_of(ui));
    }

    pub(crate) fn export_notes_markdown(&mut self) {
        if self.notes_page.rows.is_empty() {
            self.notes_page_rebuild();
        }
        let selected = &self.notes_page.selected;
        let rows: Vec<Row> = self
            .notes_page
            .rows
            .iter()
            .filter(|r| selected.is_empty() || selected.contains(&format!("{}:{}", r.book_id, r.id)))
            .cloned()
            .collect();
        if rows.is_empty() {
            return;
        }
        let mut md = String::from("# 笔记导出\n\n");
        let mut book = String::new();
        for r in rows {
            if r.book_title != book {
                md.push_str(&format!("\n## {}\n\n", r.book_title));
                book = r.book_title.clone();
            }
            md.push_str(&format!("> {}\n\n", r.text.trim()));
            if !r.note.is_empty() {
                md.push_str(&format!("笔记：{}\n\n", r.note));
            }
            if !r.tags.is_empty() {
                md.push_str(&format!(
                    "标签：{}\n\n",
                    r.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" ")
                ));
            }
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("notes.md")
            .add_filter("Markdown", &["md"])
            .save_file()
        {
            let _ = std::fs::write(path, md);
        }
    }

    fn notes_page_load_tags(&mut self) {
        if self.notes_page.tag_loaded {
            return;
        }
        let path = std::path::Path::new(&self.data_dir).join("tags.json");
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(list) = serde_json::from_str::<Vec<String>>(&text) {
                self.notes_page.tags = list;
            }
        }
        self.notes_page.tag_loaded = true;
    }

    fn notes_page_save_tags(&mut self) {
        let path = std::path::Path::new(&self.data_dir).join("tags.json");
        let _ = std::fs::write(
            path,
            serde_json::to_string(&self.notes_page.tags).unwrap_or_else(|_| "[]".into()),
        );
    }

    fn notes_page_rebuild(&mut self) {
        self.notes_page.dirty = false;
        let query = self.notes_page.query.to_lowercase();
        let books = self.library.books.clone();
        let mut rows = Vec::new();
        for book in books {
            let Some(config) =
                reader_core::library::Library::read_book_config(&self.data_dir, &book.id)
            else {
                continue;
            };
            let cached = self
                .notes_page
                .text_cache
                .entry(book.id.clone())
                .or_insert_with(|| build_text_cache(&book.path));
            for h in &config.highlights {
                let cached_text = cached
                    .get(&(h.chapter, h.start_block))
                    .cloned()
                    .unwrap_or_default();
                let text = if h.text.is_empty() {
                    cached_text
                } else {
                    h.text.clone()
                };
                let note = config
                    .notes
                    .iter()
                    .find(|n| n.highlight_id == h.id)
                    .map(|n| n.content.clone())
                    .unwrap_or_default();
                if self.notes_page.untagged && !h.tags.is_empty() {
                    continue;
                }
                if !query.is_empty() {
                    let hay = format!("{} {} {}", text, note, book.title).to_lowercase();
                    if !query.split_whitespace().all(|w| hay.contains(w)) {
                        continue;
                    }
                }
                rows.push(Row {
                    id: h.id.clone(),
                    book_id: book.id.clone(),
                    book_title: book.title.clone(),
                    path: book.path.clone(),
                    chapter: h.chapter,
                    chapter_title: String::new(),
                    block: h.start_block,
                    text,
                    note,
                    tags: h.tags.clone(),
                });
            }
        }
        rows.sort_by(|a, b| {
            (a.book_title.clone(), a.chapter, a.block).cmp(&(b.book_title.clone(), b.chapter, b.block))
        });
        self.notes_page.rows = rows;
    }

    fn notes_page_modals(&mut self, ctx: &egui::Context) {
        if !self.notes_page.confirm_batch.is_empty() {
            let mut open = true;
            let mut yes = false;
            let count = self.notes_page.confirm_batch.len();
            let mut take_focus = self.notes_page.confirm_focus;
            egui::Window::new("删除笔记")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(format!("确认删除选中的 {count} 条笔记？"));
                    ui.horizontal(|ui| {
                        let del = ui.button("删除");
                        if take_focus {
                            del.request_focus();
                            take_focus = false;
                        }
                        if del.clicked() {
                            yes = true;
                        }
                        if ui.button("取消").clicked() {
                            self.notes_page.confirm_batch.clear();
                        }
                    });
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        yes = true;
                    }
                });
            if yes {
                let list = self.notes_page.confirm_batch.clone();
                for (book_id, highlight_id) in list {
                    if let Some(mut cfg) =
                        reader_core::library::Library::read_book_config(&self.data_dir, &book_id)
                    {
                        cfg.highlights.retain(|h| h.id != highlight_id);
                        let alive: std::collections::HashSet<String> =
                            cfg.highlights.iter().map(|h| h.id.clone()).collect();
                        cfg.notes.retain(|n| alive.contains(&n.highlight_id));
                        let _ = cfg.save(&self.data_dir);
                    }
                    self.notes_page
                        .selected
                        .remove(&format!("{book_id}:{highlight_id}"));
                }
                self.notes_page.confirm_batch.clear();
                self.notes_page.confirm_focus = false;
                self.notes_page.dirty = true;
            } else if !open {
                self.notes_page.confirm_batch.clear();
            }
        }

        if let Some((book_id, highlight_id)) = self.notes_page.confirm.clone() {
            let mut open = true;
            let mut yes = false;
            egui::Window::new("删除笔记")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label("确认删除这条笔记？");
                    ui.horizontal(|ui| {
                        yes = ui.button("删除").clicked();
                        if ui.button("取消").clicked() {
                            self.notes_page.confirm = None;
                        }
                    });
                });
            if yes {
                if let Some(mut cfg) =
                    reader_core::library::Library::read_book_config(&self.data_dir, &book_id)
                {
                    cfg.highlights.retain(|h| h.id != highlight_id);
                    let alive: std::collections::HashSet<String> =
                        cfg.highlights.iter().map(|h| h.id.clone()).collect();
                    cfg.notes.retain(|n| alive.contains(&n.highlight_id));
                    let _ = cfg.save(&self.data_dir);
                }
                self.notes_page.confirm = None;
                self.notes_page.dirty = true;
            } else if !open {
                self.notes_page.confirm = None;
            }
        }

        if let Some((book_id, block, mut buf)) = self.notes_page.tag_target.clone() {
            let mut close = false;
            let mut apply = false;
            egui::Window::new("标签")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut buf));
                    ui.horizontal(|ui| {
                        apply = ui.button("添加").clicked();
                        close = ui.button("取消").clicked();
                    });
                });
            if let Some(t) = self.notes_page.tag_target.as_mut() {
                t.2 = buf.clone();
            }
            if apply && !buf.trim().is_empty() {
                let tag = buf.trim().to_string();
                if resave_highlight_tags(&self.data_dir, &book_id, &block, &tag) {
                    if !self.notes_page.tags.contains(&tag) {
                        self.notes_page.tags.push(tag);
                        self.notes_page_save_tags();
                    }
                    self.notes_page.dirty = true;
                }
                self.notes_page.tag_target = None;
            } else if close {
                self.notes_page.tag_target = None;
            }
        }
    }
}

fn build_text_cache(path: &str) -> HashMap<(usize, usize), String> {
    let mut out = HashMap::new();
    let Ok(book) = reader_core::epub::EpubBook::open_lazy(path) else {
        return out;
    };
    for (ci, chapter) in book.chapters.iter().enumerate() {
        for (bi, block) in chapter.blocks.iter().enumerate() {
            let text: String = match block {
                reader_core::epub::ContentBlock::Heading { spans, .. }
                | reader_core::epub::ContentBlock::Paragraph { spans, .. } => {
                    spans.iter().map(|s| s.text.as_str()).collect()
                }
                _ => continue,
            };
            out.insert((ci, bi), text);
        }
    }
    out
}

fn resave_highlight_tags(data_dir: &str, book_id: &str, highlight_id: &str, tag: &str) -> bool {
    let Some(mut cfg) = reader_core::library::Library::read_book_config(data_dir, book_id) else {
        return false;
    };
    let mut changed = false;
    for h in cfg.highlights.iter_mut().filter(|h| h.id == highlight_id) {
        if !h.tags.iter().any(|t| t == tag) {
            h.tags.push(tag.to_string());
            changed = true;
        }
    }
    changed && cfg.save(data_dir).is_ok()
}

fn ctx_of(ui: &mut egui::Ui) -> &egui::Context {
    ui.ctx()
}
