//! 高亮后的「添加备注」弹窗（仿 BN），Esc 关闭，Ctrl+Enter 保存。
use crate::app::ReaderApp;
use eframe::egui;

pub struct NotePopup {
    pub highlight_id: String,
    pub buffer: String,
    pub preview: String,
    pub focus: bool,
    pub tag_buf: String,
    pub book_id: Option<String>,
    pub tag_focus: bool,
}

impl ReaderApp {
    pub(crate) fn render_note_popup(&mut self, ctx: &egui::Context) {
        let Some(popup) = self.note_popup.as_ref() else {
            return;
        };
        let highlight_id = popup.highlight_id.clone();
        let preview = popup.preview.clone();
        let mut buffer = popup.buffer.clone();
        let mut close = false;
        let mut tag_buf = popup.tag_buf.clone();
        let mut add_tag: Option<String> = None;
        let book_id = popup.book_id.clone();
        let take_focus = popup.focus;
        let take_tag_focus = popup.tag_focus;
        let mut save = false;

        egui::Window::new("笔记")
            .id(egui::Id::new("note_popup"))
            .collapsible(false)
            .resizable(true)
            .default_width(420.)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(preview.as_str()).weak());
                ui.add_space(4.);
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut buffer)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
                if take_focus {
                    resp.request_focus();
                }
                ui.add_space(6.);
                ui.horizontal(|ui| {
                    ui.label("标签");
                    let tag_resp = ui.add(egui::TextEdit::singleline(&mut tag_buf).desired_width(120.));
                    if take_tag_focus { tag_resp.request_focus(); }
                    if ui.button("添加").clicked() && !tag_buf.trim().is_empty() {
                        add_tag = Some(tag_buf.trim().to_string());
                    }
                });
                ui.add_space(4.);
                ui.horizontal(|ui| {
                    if ui.button("保存").clicked() {
                        save = true;
                    }
                    if ui.button("取消").clicked() {
                        close = true;
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
                if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) {
                    save = true;
                }
            });

        if let Some(popup) = self.note_popup.as_mut() {
            popup.buffer = buffer;
            popup.tag_buf = tag_buf;
            popup.focus = false;
            popup.tag_focus = false;
        }
        if let Some(tag) = add_tag {
            self.attach_tag(book_id.as_deref(), &highlight_id, &tag);
        }
        if save {
            self.save_note_popup(&highlight_id);
        } else if close {
            self.note_popup = None;
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
        }
    }

    fn attach_tag(&mut self, book_id: Option<&str>, highlight_id: &str, tag: &str) {
        let mut cfg = match book_id {
            Some(id) => match reader_core::library::Library::read_book_config(&self.data_dir, id) {
                Some(cfg) => cfg,
                None => return,
            },
            None => match self.book_config.clone() {
                Some(cfg) => cfg,
                None => return,
            },
        };
        let mut changed = false;
        for h in cfg.highlights.iter_mut().filter(|h| h.id == highlight_id) {
            if !h.tags.iter().any(|t| t == tag) {
                h.tags.push(tag.to_string());
                changed = true;
            }
        }
        if changed && cfg.save(&self.data_dir).is_ok() {
            if book_id.is_none() {
                self.book_config = Some(cfg);
            }
            let path = std::path::Path::new(&self.data_dir).join("tags.json");
            let mut tags: Vec<String> = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            if !tags.iter().any(|t| t == tag) {
                tags.push(tag.to_string());
                let _ = std::fs::write(path, serde_json::to_string(&tags).unwrap_or_default());
            }
        }
    }

    pub(crate) fn save_note_popup_current(&mut self) {
        let Some(id) = self.note_popup.as_ref().map(|p| p.highlight_id.clone()) else {
            return;
        };
        self.save_note_popup(&id);
    }

    fn save_note_popup(&mut self, highlight_id: &str) {
        let target = self
            .note_popup
            .as_ref()
            .and_then(|p| p.book_id.clone())
            .or_else(|| self.book_config.as_ref().map(|c| c.id.clone()));
        let Some(book) = target else {
            self.note_popup = None;
            return;
        };
        let buffer = self
            .note_popup
            .as_ref()
            .map(|p| p.buffer.clone())
            .unwrap_or_default();
        if buffer.trim().is_empty() {
            self.note_popup = None;
            return;
        }
        match reader_core::annotations::save_note(
            &self.data_dir,
            &book,
            highlight_id,
            &buffer,
            None,
        ) {
            Ok(config) => {
                if self.book_config.as_ref().is_some_and(|c| c.id == config.id) {
                    self.book_config = Some(config);
                }
            }
            Err(error) => self.error_msg = Some(error),
        }
        self.note_popup = None;
    }
}
