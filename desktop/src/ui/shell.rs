use super::design::{self, Icon, Palette};
use crate::app::{AppView, ReaderApp};
use eframe::egui;
impl ReaderApp {
    pub fn render_app_navigation(&mut self, ctx: &egui::Context) {
        let p = Palette::new(self.dark_mode);
        if !self.show_app_navigation {
            egui::TopBottomPanel::top("app_navigation_reopen")
                .frame(egui::Frame::default().fill(p.surface).inner_margin(8.))
                .show(ctx, |ui| {
                    if ui.button(self.i18n.t("ui.show_navigation")).clicked() {
                        self.show_app_navigation = true;
                    }
                });
            return;
        }
        let font = egui::TextStyle::Button.resolve(&ctx.style());
        let text_color = ctx.style().visuals.text_color();
        let label_width = ctx.fonts(|fonts| {
            [
                "library.title",
                "ui.return_reader",
                "settings.title",
            ]
            .iter()
            .map(|key| {
                fonts
                    .layout_no_wrap(self.i18n.t(key).to_owned(), font.clone(), text_color)
                    .size()
                    .x
            })
            .fold(0_f32, f32::max)
        });
        let navigation_width =
            (label_width + 58. + 2. * f32::from(design::Geometry::CARD_INSET)).clamp(196., 260.);
        egui::SidePanel::left("app_navigation")
            .exact_width(navigation_width)
            .resizable(false)
            .frame(
                egui::Frame::default()
                    .fill(p.surface)
                    .inner_margin(design::Geometry::CARD_INSET),
            )
            .show(ctx, |ui| {
                let panel_rect = ui
                    .max_rect()
                    .expand(f32::from(design::Geometry::CARD_INSET));
                ui.set_max_width(ui.available_width() - 16.);
                ui.add_space(12.);
                design::sidebar_title(ui, "EpubReader");
                ui.add_space(32.);
                for (key, icon, selected) in [
                    ("library.title", Icon::Book, self.view == AppView::Library),
                    ("notes.title", Icon::Notes, self.view == AppView::Notes),
                ] {
                    let response = design::navigation_item(ui, icon, self.i18n.t(key), selected);
                    if response.clicked() {
                        if key == "library.title" {
                            if self.view != AppView::Library {
                                self.book_presentation.shelf_order.reset();
                            }
                            self.view = AppView::Library;
                        } else {
                            self.view = AppView::Notes;
                            self.notes_page.dirty = true;
                            self.notes_page.focus = true;
                        }
                    }
                    ui.add_space(4.);
                }
                if self.book.is_some() {
                    ui.add_space(16.);
                    if design::navigation_item(
                        ui,
                        Icon::Back,
                        self.i18n.t("ui.return_reader"),
                        false,
                    )
                    .clicked()
                    {
                        self.view = AppView::Reader;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    let response = super::design::menu_button(
                        ui,
                        egui::Button::new("")
                            .min_size(egui::vec2(
                                ui.available_width(),
                                design::Geometry::NAV_HEIGHT,
                            ))
                            .stroke(egui::Stroke::NONE),
                        |ui| {
                            design::menu_style(ui);
                            if design::menu_item(ui, self.i18n.t("about.title"), false, false)
                                .clicked()
                            {
                                self.show_about = true;
                                ui.memory_mut(|m| m.close_popup());
                            }
                            if design::menu_item(
                                ui,
                                self.i18n.t(if self.dark_mode {
                                    "toolbar.light_mode"
                                } else {
                                    "toolbar.dark_mode"
                                }),
                                false,
                                false,
                            )
                            .clicked()
                            {
                                self.appearance_linkage.set_app(!self.dark_mode);
                                ui.memory_mut(|m| m.close_popup());
                            }
                            if design::menu_item(
                                ui,
                                self.i18n.t("appearance.link_title"),
                                false,
                                false,
                            )
                            .clicked()
                            {
                                self.show_appearance_linkage = true;
                                ui.memory_mut(|m| m.close_popup());
                            }
                            ui.separator();
                            let language_label = format!(
                                "{} · {}",
                                self.i18n.t("language.title"),
                                self.i18n.choice_label(self.i18n.preference())
                            );
                            if design::menu_item(ui, &language_label, false, false).clicked() {
                                self.show_language = true;
                                ui.memory_mut(|m| m.close_popup());
                            }
                        },
                    );
                    design::paint_navigation_item(
                        ui,
                        &response.response,
                        Icon::Settings,
                        self.i18n.t("settings.title"),
                        false,
                    );
                });
                if design::sidebar_collapse_button(
                    ui,
                    panel_rect,
                    true,
                    self.i18n.t("common.collapse"),
                )
                .clicked()
                {
                    self.show_app_navigation = false;
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_footer_fits_small_window_in_both_languages() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(directory.path().to_string_lossy().into_owned());
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../core/tests/fixtures/illustrations.epub");
        app.book = Some(reader_core::epub::EpubBook::open(fixture.to_str().unwrap()).unwrap());
        fn collect(
            shape: &egui::Shape,
            result: &mut std::collections::HashMap<String, (egui::Rect, bool)>,
        ) {
            match shape {
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, result)
                    }
                }
                egui::Shape::Text(text) => {
                    result.insert(
                        text.galley.job.text.clone(),
                        (text.visual_bounding_rect(), text.galley.elided),
                    );
                }
                _ => {}
            }
        }
        for language in [
            reader_core::i18n::Language::ZhCN,
            reader_core::i18n::Language::En,
        ] {
            app.i18n = reader_core::i18n::I18n::with_system(language, "en");
            let ctx = egui::Context::default();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 400.),
                    )),
                    ..Default::default()
                },
                |ctx| app.render_app_navigation(ctx),
            );
            let mut labels = std::collections::HashMap::new();
            for shape in output.shapes {
                collect(&shape.shape, &mut labels)
            }
            let resume = labels[app.i18n.t("ui.return_reader")].0;
            let settings = labels[app.i18n.t("settings.title")].0;
            assert!(
                settings.min.y > resume.max.y + 16.,
                "Settings must belong to the footer"
            );
            assert!(settings.max.y < 392.);
            for key in [
                "library.title",
                "ui.return_reader",
                "settings.title",
            ] {
                let (rect, elided) = labels[app.i18n.t(key)];
                assert!(!elided, "Navigation label truncated: {key}");
                assert!(rect.min.x >= 0. && rect.max.x < 260.);
            }
        }
    }
}
