//! 高亮备注弹窗：编辑备注与标签、删除高亮，Esc 关闭，Ctrl+Enter 保存。
use crate::app::ReaderApp;
use eframe::egui;

pub struct NotePopup {
    pub highlight_id: String,
    pub buffer: String,
    pub preview: String,
    pub focus: bool,
    pub tag_buf: String,
    pub tags: Vec<String>,
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
        let tags = popup.tags.clone();
        let mut add_tag: Option<String> = None;
        let mut remove_tag: Option<String> = None;
        let mut delete = false;
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
                let max_height = (ctx.screen_rect().height() - 100.0).max(180.0);
                egui::ScrollArea::vertical()
                    .max_height(max_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(preview.as_str()).weak());
                        ui.add_space(4.);
                        let resp = ui.add(
                            egui::TextEdit::multiline(&mut buffer)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        );
                        if take_focus {
                            resp.request_focus();
                        }
                        ui.add_space(6.);
                        ui.horizontal_wrapped(|ui| {
                            ui.label("标签");
                            if tags.is_empty() {
                                ui.weak("无");
                            }
                            for tag in &tags {
                                if ui.small_button(format!("{tag} ×")).clicked() {
                                    remove_tag = Some(tag.clone());
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            let tag_resp = ui.add(
                                egui::TextEdit::singleline(&mut tag_buf)
                                    .hint_text("输入标签后回车")
                                    .desired_width(160.),
                            );
                            if take_tag_focus {
                                tag_resp.request_focus();
                            }
                            let submitted = (tag_resp.has_focus() || tag_resp.lost_focus())
                                && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if (ui.button("添加").clicked() || submitted)
                                && !tag_buf.trim().is_empty()
                            {
                                add_tag = Some(tag_buf.trim().to_string());
                            }
                        });
                        ui.add_space(6.);
                        ui.horizontal(|ui| {
                            if ui.button("保存").clicked() {
                                save = true;
                            }
                            if ui.button("删除高亮").clicked() {
                                delete = true;
                            }
                            if ui.button("取消").clicked() {
                                close = true;
                            }
                        });
                    });
            });

        if let Some(popup) = self.note_popup.as_mut() {
            popup.buffer = buffer;
            popup.tag_buf = tag_buf;
            popup.focus = false;
            popup.tag_focus = false;
        }
        if let Some(tag) = add_tag {
            self.attach_tag(book_id.as_deref(), &highlight_id, &tag);
            if let Some(popup) = self.note_popup.as_mut() {
                if !popup.tags.iter().any(|t| t == &tag) {
                    popup.tags.push(tag);
                }
                popup.tag_buf.clear();
            }
        }
        if let Some(tag) = remove_tag {
            self.detach_tag(book_id.as_deref(), &highlight_id, &tag);
            if let Some(popup) = self.note_popup.as_mut() {
                popup.tags.retain(|t| t != &tag);
            }
        }
        if delete {
            self.delete_highlight(book_id.as_deref(), &highlight_id);
            close = true;
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

    fn config_for(&self, book_id: Option<&str>) -> Option<reader_core::library::BookConfig> {
        match book_id {
            Some(id) => reader_core::library::Library::read_book_config(&self.data_dir, id),
            None => self.book_config.clone(),
        }
    }

    fn store_config(&mut self, book_id: Option<&str>, cfg: reader_core::library::BookConfig) {
        if book_id.is_none() {
            self.book_config = Some(cfg);
        }
    }

    fn attach_tag(&mut self, book_id: Option<&str>, highlight_id: &str, tag: &str) {
        let Some(mut cfg) = self.config_for(book_id) else {
            return;
        };
        let mut changed = false;
        for h in cfg.highlights.iter_mut().filter(|h| h.id == highlight_id) {
            if !h.tags.iter().any(|t| t == tag) {
                h.tags.push(tag.to_string());
                changed = true;
            }
        }
        if changed && cfg.save(&self.data_dir).is_ok() {
            self.store_config(book_id, cfg);
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

    fn detach_tag(&mut self, book_id: Option<&str>, highlight_id: &str, tag: &str) {
        let Some(mut cfg) = self.config_for(book_id) else {
            return;
        };
        let mut changed = false;
        for h in cfg.highlights.iter_mut().filter(|h| h.id == highlight_id) {
            let before = h.tags.len();
            h.tags.retain(|t| t != tag);
            changed |= h.tags.len() != before;
        }
        if changed && cfg.save(&self.data_dir).is_ok() {
            self.store_config(book_id, cfg);
        }
    }

    fn delete_highlight(&mut self, book_id: Option<&str>, highlight_id: &str) {
        let Some(mut cfg) = self.config_for(book_id) else {
            return;
        };
        cfg.highlights.retain(|h| h.id != highlight_id);
        cfg.notes.retain(|n| n.highlight_id != highlight_id);
        if let Err(error) = cfg.save(&self.data_dir) {
            self.error_msg = Some(error.to_string());
            return;
        }
        self.store_config(book_id, cfg);
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
