//! Ctrl+P 快速打开书籍（VSCode 风格：输入过滤 + 上下键 + Enter）。
use crate::app::ReaderApp;
use eframe::egui;

#[derive(Default)]
pub(crate) struct QuickOpen {
    pub open: bool,
    pub query: String,
    pub selected: usize,
    pub focus: bool,
}

impl ReaderApp {
    pub(crate) fn quick_open_entries(&self) -> Vec<usize> {
        let q = self.quick_open.query.trim().to_lowercase();
        self.library
            .books
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                q.is_empty()
                    || b.title.to_lowercase().contains(&q)
                    || b.path.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub(crate) fn render_quick_open(&mut self, ctx: &egui::Context) {
        if !self.quick_open.open {
            return;
        }
        let entries = self.quick_open_entries();
        if self.quick_open.selected >= entries.len() && !entries.is_empty() {
            self.quick_open.selected = entries.len() - 1;
        }
        let mut open = true;
        let mut chosen: Option<usize> = None;
        egui::Window::new("打开书籍")
            .id(egui::Id::new("quick_open"))
            .collapsible(false)
            .resizable(false)
            .default_width(520.)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 120.0))
            .open(&mut open)
            .show(ctx, |ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.quick_open.query)
                        .desired_width(f32::INFINITY),
                );
                if self.quick_open.focus {
                    resp.request_focus();
                    self.quick_open.focus = false;
                }
                if resp.changed() {
                    self.quick_open.selected = 0;
                }
                ui.separator();
                egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
                    for (n, idx) in entries.iter().enumerate() {
                        let book = &self.library.books[*idx];
                        let label = format!("{}", book.title);
                        if ui
                            .selectable_label(n == self.quick_open.selected, label)
                            .clicked()
                        {
                            chosen = Some(*idx);
                        }
                    }
                    if entries.is_empty() {
                        ui.label("无匹配书籍");
                    }
                });
            });
        self.quick_open.open = open;
        if let Some(idx) = chosen {
            if let Some(book) = self.library.books.get(idx).cloned() {
                self.quick_open.open = false;
                self.open_book_from_path(&book.path, Some(book.last_chapter));
            }
        }
    }
}

impl ReaderApp {
    pub(crate) fn render_bookmark_toast(&mut self, ctx: &egui::Context) {
        let Some((msg, until)) = self.bookmark_toast.clone() else {
            return;
        };
        let now = ctx.input(|i| i.time);
        if now >= until {
            self.bookmark_toast = None;
            return;
        }
        let alpha = ((until - now) / 1.2).clamp(0.0, 1.0) as f32;
        egui::Area::new(egui::Id::new("bookmark_toast"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 96.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_opacity(alpha);
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(egui::RichText::new(msg).strong());
                });
            });
        ctx.request_repaint();
    }
}
