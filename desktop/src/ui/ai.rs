//! Right-side AI assistant panel with OpenAI-compatible streaming chat.
use crate::app::ReaderApp;
use eframe::egui;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct AiMessage {
    pub role: &'static str,
    pub content: String,
}

pub(crate) enum AiEvent {
    Chunk(String),
    Done,
    Error(String),
}

struct AiConfig {
    endpoint: String,
    key: String,
    model: String,
}

fn endpoint(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

fn start_request(
    cfg: AiConfig,
    messages: Vec<AiMessage>,
    stop: Arc<AtomicBool>,
) -> std::sync::mpsc::Receiver<AiEvent> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .map_err(|e| e.to_string())?;
            let payload = serde_json::json!({
                "model": cfg.model,
                "stream": true,
                "messages": messages
                    .iter()
                    .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
                    .collect::<Vec<_>>(),
            });
            let response = client
                .post(&cfg.endpoint)
                .bearer_auth(&cfg.key)
                .json(&payload)
                .send()
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().unwrap_or_default();
                return Err(format!(
                    "HTTP {}: {}",
                    status.as_u16(),
                    body.chars().take(400).collect::<String>()
                ));
            }
            for line in BufReader::new(response).lines() {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let line = line.map_err(|e| e.to_string())?;
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data == "[DONE]" {
                    break;
                }
                if data.is_empty() {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
                    continue;
                };
                if let Some(delta) = value["choices"][0]["delta"]["content"].as_str() {
                    if !delta.is_empty() && tx.send(AiEvent::Chunk(delta.to_string())).is_err() {
                        break;
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                let _ = tx.send(AiEvent::Done);
            }
            Err(error) => {
                let _ = tx.send(AiEvent::Error(error));
            }
        }
    });
    rx
}

impl ReaderApp {
    pub(crate) fn ai_poll(&mut self) {
        let Some(rx) = self.ai_rx.as_ref() else {
            return;
        };
        let mut finished = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                AiEvent::Chunk(text) => {
                    if let Some(last) = self.ai_messages.last_mut() {
                        last.content.push_str(&text);
                    }
                }
                AiEvent::Done => finished = true,
                AiEvent::Error(error) => {
                    self.ai_status = error;
                    finished = true;
                }
            }
        }
        if finished {
            self.ai_rx = None;
            self.ai_streaming = false;
            self.ai_scroll_to_end = true;
        }
    }

    fn ai_send(&mut self) {
        if self.ai_streaming || self.ai_input.trim().is_empty() {
            return;
        }
        let endpoint = endpoint(&self.ai_base_url);
        if endpoint.is_empty() || self.ai_api_key.is_empty() || self.ai_model.trim().is_empty() {
            self.ai_status = self.i18n.t("ai.not_configured").into();
            return;
        }
        let text = self.ai_input.trim().to_string();
        self.ai_input.clear();
        self.ai_messages.push(AiMessage {
            role: "user",
            content: text,
        });
        self.ai_messages.push(AiMessage {
            role: "assistant",
            content: String::new(),
        });
        let mut history = vec![AiMessage {
            role: "system",
            content: self.i18n.t("ai.system_prompt").into(),
        }];
        history.extend(self.ai_messages.iter().take(self.ai_messages.len() - 1).cloned());
        self.ai_status.clear();
        self.ai_stop = Arc::new(AtomicBool::new(false));
        self.ai_streaming = true;
        self.ai_scroll_to_end = true;
        self.ai_rx = Some(start_request(
            AiConfig {
                endpoint,
                key: self.ai_api_key.clone(),
                model: self.ai_model.clone(),
            },
            history,
            self.ai_stop.clone(),
        ));
    }

    pub(crate) fn open_ai_with_selection(&mut self, text: String) {
        self.show_ai_panel = true;
        self.show_annotations = false;
        self.ai_input = text;
    }

    pub(crate) fn render_ai_panel(&mut self, ctx: &egui::Context) {
        self.ai_poll();
        let collapse = self.i18n.t("common.collapse").to_owned();
        let width = (ctx.screen_rect().width() * 0.32)
            .clamp(260., 380.)
            .min(ctx.screen_rect().width() * 0.5);
        let open = self.show_ai_panel && self.view == crate::app::AppView::Reader;
        if crate::ui::design::animated_side_panel(
            ctx,
            "ai_panel",
            false,
            width,
            open,
            &collapse,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(self.i18n.t("ai.title")).size(17.).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").on_hover_text(self.i18n.t("common.collapse")).clicked() {
                            self.show_ai_panel = false;
                        }
                        if ui.small_button(self.i18n.t("ai.clear")).clicked() {
                            self.ai_messages.clear();
                            self.ai_status.clear();
                        }
                    });
                });
                ui.separator();
                if self.ai_messages.is_empty() {
                    ui.label(
                        egui::RichText::new(self.i18n.t("ai.empty"))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                let input_height = if self.ai_messages.is_empty() { 320. } else { 220. };
                egui::ScrollArea::vertical()
                    .id_salt("ai_messages")
                    .stick_to_bottom(true)
                    .max_height((ui.available_height() - input_height).max(120.))
                    .show(ui, |ui| {
                        for message in &self.ai_messages {
                            let (label, color) = if message.role == "user" {
                                (self.i18n.t("ai.you"), egui::Color32::from_gray(140))
                            } else {
                                (self.i18n.t("ai.assistant"), egui::Color32::from_gray(90))
                            };
                            ui.label(egui::RichText::new(label).small().color(color));
                            ui.add(
                                egui::Label::new(&message.content)
                                    .selectable(true)
                                    .wrap(),
                            );
                            ui.add_space(8.);
                        }
                    });
                if !self.ai_status.is_empty() {
                    ui.colored_label(egui::Color32::from_rgb(200, 80, 60), &self.ai_status);
                }
                ui.add_space(4.);
                let input = ui.add(
                    egui::TextEdit::multiline(&mut self.ai_input)
                        .id(egui::Id::new("ai_input"))
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text(self.i18n.t("ai.placeholder")),
                );
                if ui.ctx().memory(|m| m.focused()).is_none() {
                    input.request_focus();
                }
                ui.horizontal(|ui| {
                    if self.ai_streaming {
                        if ui
                            .button(egui::RichText::new(self.i18n.t("ai.stop")).color(
                                egui::Color32::from_rgb(200, 80, 60),
                            ))
                            .clicked()
                        {
                            self.ai_stop.store(true, Ordering::Relaxed);
                            self.ai_rx = None;
                            self.ai_streaming = false;
                        }
                        ui.spinner();
                        ui.label(
                            egui::RichText::new(self.i18n.t("ai.thinking"))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    } else if ui.button(self.i18n.t("ai.send")).clicked() {
                        self.ai_send();
                    }
                    if ui.small_button(self.i18n.t("ai.settings")).clicked() {
                        self.show_settings = true;
                    }
                });
                if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter)) {
                    self.ai_send();
                }
            },
        ) {
            self.show_ai_panel = false;
        }
    }
}
