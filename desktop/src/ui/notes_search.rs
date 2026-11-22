//! 跨书「笔记搜索」窗口：扫描每本书的 notes，按备注内容查找并跳转。
use crate::app::ReaderApp;
use eframe::egui;

#[derive(Clone)]
pub struct NoteHit {
    pub book_index: usize,
    pub book_title: String,
    pub chapter: usize,
    pub block: usize,
    pub text: String,
}

impl ReaderApp {
    pub(crate) fn render_notes_search(&mut self, ctx: &egui::Context) {
        if !self.show_notes_search {
            return;
        }
        let mut open = self.show_notes_search;
        let mut go: Option<NoteHit> = None;
        let mut run = false;
        egui::Window::new("笔记搜索")
            .open(&mut open)
            .default_width(520.)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.notes_search_query)
                            .desired_width(360.),
                    );
                    if self.notes_search_just_opened {
                        resp.request_focus();
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        run = true;
                    }
                    if ui.button("搜索").clicked() {
                        run = true;
                    }
                });
                ui.separator();
                if run {
                    self.run_notes_search();
                }
                egui::ScrollArea::vertical()
                    .max_height(380.)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.notes_search_results.is_empty() {
                            ui.label("无匹配");
                        }
                        for hit in self.notes_search_results.clone() {
                            let label = format!(
                                "{} | 第 {} 章 | {}",
                                hit.book_title,
                                hit.chapter + 1,
                                hit.text
                            );
                            if ui.selectable_label(false, label).clicked() {
                                go = Some(hit);
                            }
                        }
                    });
            });
        self.notes_search_just_opened = false;
        self.show_notes_search = open;
        if let Some(hit) = go {
            self.open_note_hit(hit);
        }
    }

    fn run_notes_search(&mut self) {
        let query = self.notes_search_query.trim().to_lowercase();
        let mut hits = Vec::new();
        if !query.is_empty() {
            for (index, book) in self.library.books.iter().enumerate() {
                let Some(config) =
                    reader_core::library::Library::read_book_config(&self.data_dir, &book.id)
                else {
                    continue;
                };
                // 高亮原文：按需打开 EPUB 取 block 文本
                let epub = reader_core::epub::EpubBook::open_lazy(&book.path).ok();
                for highlight in &config.highlights {
                    let text = epub
                        .as_ref()
                        .and_then(|b| b.chapters.get(highlight.chapter))
                        .and_then(|c| c.blocks.get(highlight.start_block))
                        .map(block_text)
                        .unwrap_or_default();
                    if text.to_lowercase().contains(&query) {
                        hits.push(NoteHit {
                            book_index: index,
                            book_title: book.title.clone(),
                            chapter: highlight.chapter,
                            block: highlight.start_block,
                            text: text.chars().take(120).collect(),
                        });
                    }
                }
                for note in &config.notes {
                    if !note.content.to_lowercase().contains(&query) {
                        continue;
                    }
                    let Some(highlight) = config
                        .highlights
                        .iter()
                        .find(|h| h.id == note.highlight_id)
                    else {
                        continue;
                    };
                    hits.push(NoteHit {
                        book_index: index,
                        book_title: book.title.clone(),
                        chapter: highlight.chapter,
                        block: highlight.start_block,
                        text: note.content.chars().take(120).collect(),
                    });
                }
            }
        }
        self.notes_search_results = hits;
    }

    fn open_note_hit(&mut self, hit: NoteHit) {
        let Some(book) = self.library.books.get(hit.book_index).cloned() else {
            return;
        };
        self.show_notes_search = false;
        self.open_book_from_path(&book.path, Some(hit.chapter));
        self.pending_restore_block = Some(hit.block);
    }
}

fn block_text(block: &reader_core::epub::ContentBlock) -> String {
    match block {
        reader_core::epub::ContentBlock::Heading { spans, .. }
        | reader_core::epub::ContentBlock::Paragraph { spans, .. } => {
            spans.iter().map(|s| s.text.as_str()).collect()
        }
        _ => String::new(),
    }
}
