use egui::{Color32, FontId, Key, TextEdit, text_selection::CCursorRange};
use egui::text::{CCursor, LayoutJob, TextFormat};

use crate::config::{parse_hex_color, Config};
use crate::filter::{filter_apps, FilteredApp};
use crate::frequency::Frequency;
use crate::launcher;

fn build_highlighted_text(
    name: &str,
    match_indices: &[usize],
    base_color: Color32,
    highlight_color: Color32,
    font_id: FontId,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (i, ch) in name.chars().enumerate() {
        let color = if match_indices.contains(&i) {
            highlight_color
        } else {
            base_color
        };
        job.append(
            &ch.to_string(),
            0.0,
            TextFormat {
                font_id: font_id.clone(),
                color,
                ..Default::default()
            },
        );
    }
    job
}

pub struct LauncherApp {
    query: String,
    apps: Vec<String>,
    frequency: Frequency,
    selected: usize,
    scroll_offset: usize,
    last_visible: usize,
    page_size: usize,
    fg: Color32,
    bg: Color32,
    sel_fg: Color32,
    sel_bg: Color32,
    match_hl: Color32,
    prompt_color: Color32,
    font_size: f32,
    first_frame: bool,
    should_close: bool,
    request_focus: bool,
    delete_confirm: Option<String>,
    cursor_in_results: bool,
    close_at: Option<std::time::Instant>,
    hide_window: bool,
}

impl LauncherApp {
    pub fn new(config: Config, apps: Vec<String>, frequency: Frequency) -> Self {
        let fg = parse_hex_color(&config.appearance.foreground).unwrap_or(Color32::WHITE);
        let bg = parse_hex_color(&config.appearance.background).unwrap_or(Color32::BLACK);
        let sel_fg = parse_hex_color(&config.appearance.selection_fg).unwrap_or(Color32::WHITE);
        let sel_bg = parse_hex_color(&config.appearance.selection_bg).unwrap_or(Color32::BLUE);
        let match_hl = parse_hex_color(&config.appearance.match_highlight).unwrap_or(Color32::GREEN);
        let prompt_color = parse_hex_color(&config.appearance.prompt_color).unwrap_or(Color32::from_rgb(189, 147, 249));
        let font_size = (config.appearance.font_size as f32) * (96.0 / 72.0);
        Self {
            query: String::new(),
            apps,
            frequency,
            selected: 0,
            scroll_offset: 0,
            last_visible: 0,
            page_size: 0,
            fg,
            bg,
            sel_fg,
            sel_bg,
            match_hl,
            prompt_color,
            font_size,
            first_frame: true,
            should_close: false,
            request_focus: false,
            delete_confirm: None,
            cursor_in_results: false,
            close_at: None,
            hide_window: false,
        }
    }

    pub fn should_hide(&self) -> bool {
        self.hide_window
    }

    pub fn should_quit(&self) -> bool {
        self.should_close || self.close_at.map(|t| std::time::Instant::now() >= t).unwrap_or(false)
    }

    fn launch_selected(&mut self, results: &[FilteredApp]) {
        let command = if let Some(app) = results.get(self.selected) {
            app.name.clone()
        } else if !self.query.trim().is_empty() {
            self.query.clone()
        } else {
            self.should_close = true;
            return;
        };

        self.hide_window = true;
        let result = launcher::launch_command(&command);
        if result.success && !result.command.is_empty() {
            self.frequency.increment(&result.command);
            let _ = self.frequency.save();
        }
        self.close_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
    }

    fn complete_selected(&mut self, results: &[FilteredApp]) {
        if let Some(app) = results.get(self.selected) {
            self.query = app.name.clone();
            self.request_focus = true;
        }
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        let results = filter_apps(&self.apps, &self.query, &self.frequency);
        if self.selected >= results.len() {
            self.selected = results.len().saturating_sub(1);
        }

        if self.delete_confirm.is_some() {
            if ctx.input(|i| i.key_pressed(Key::Y) || i.key_pressed(Key::Enter)) {
                if let Some(name) = self.delete_confirm.take() {
                    self.frequency.remove(&name);
                    let _ = self.frequency.save();
                    self.request_focus = true;
                }
            } else if ctx.input(|i| i.key_pressed(Key::N) || i.key_pressed(Key::Escape)) {
                self.delete_confirm = None;
                self.request_focus = true;
            }
        } else {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.should_close = true;
            }
            if ctx.input(|i| i.key_pressed(Key::Enter)) {
                self.launch_selected(&results);
            }
            if ctx.input(|i| i.key_pressed(Key::Tab)) {
                self.complete_selected(&results);
            }
            if ctx.input(|i| i.key_pressed(Key::Delete)) {
                if let Some(app) = results.get(self.selected) {
                    if self.frequency.get(&app.name) > 0 {
                        self.delete_confirm = Some(app.name.clone());
                    }
                }
            }
        }

        if self.close_at.is_some() {
            return;
        }

        let panel_frame = egui::Frame::new()
            .fill(self.bg)
            .inner_margin(egui::Margin::symmetric(4, 0));

        egui::CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
            ui.style_mut().visuals.text_cursor.blink = false;
            ui.style_mut().interaction.selectable_labels = false;

            if ctx.input(|i| i.pointer.any_click()) {
                self.request_focus = true;
            }

            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;

                if let Some(ref name) = self.delete_confirm {
                    let prompt = format!("Delete '{}'? (y/n)", name);
                    ui.label(egui::RichText::new(prompt)
                        .font(FontId::monospace(self.font_size))
                        .color(self.prompt_color));
                    return;
                }

                let prompt_width = ui.available_width() / 4.0;

                let text_edit_id = egui::Id::new("query_input");

                let cursor_at_end = if let Some(state) = TextEdit::load_state(ctx, text_edit_id) {
                    if let Some(range) = state.cursor.char_range() {
                        range.primary.index >= self.query.len()
                    } else {
                        true
                    }
                } else {
                    true
                };

                let right_pressed = ctx.input(|i| i.key_pressed(Key::ArrowRight));
                let left_pressed = ctx.input(|i| i.key_pressed(Key::ArrowLeft));

                if right_pressed && cursor_at_end && !self.cursor_in_results && results.len() > 1 {
                    self.cursor_in_results = true;
                    self.selected = 1;
                } else if self.cursor_in_results && right_pressed {
                    if self.selected < self.last_visible {
                        self.selected += 1;
                    } else if self.last_visible + 1 < results.len() {
                        self.scroll_offset = self.last_visible + 1;
                        self.selected = self.scroll_offset;
                    }
                } else if self.cursor_in_results && left_pressed {
                    if self.selected > 0 {
                        if self.selected == self.scroll_offset && self.scroll_offset > 0 {
                            let prev_page_start = self.scroll_offset.saturating_sub(self.page_size.max(1));
                            self.scroll_offset = prev_page_start;
                        }
                        self.selected -= 1;
                    } else {
                        self.cursor_in_results = false;
                    }
                }

                let keep_cursor_at_end = self.cursor_in_results;

                let mut text_edit = TextEdit::singleline(&mut self.query)
                    .id(text_edit_id)
                    .font(FontId::monospace(self.font_size))
                    .text_color(self.prompt_color)
                    .frame(false)
                    .desired_width(prompt_width);
                if self.first_frame {
                    text_edit = text_edit.cursor_at_end(true);
                }
                let response = ui.add(text_edit);
                if self.first_frame || self.request_focus {
                    response.request_focus();
                    if self.request_focus {
                        let cursor = CCursor::new(self.query.len());
                        let selection = CCursorRange::one(cursor);
                        if let Some(mut state) = TextEdit::load_state(ctx, text_edit_id) {
                            state.cursor.set_char_range(Some(selection));
                            state.store(ctx, text_edit_id);
                        }
                    }
                    self.first_frame = false;
                    self.request_focus = false;
                }

                if response.changed() {
                    self.cursor_in_results = false;
                    self.selected = 0;
                    self.scroll_offset = 0;
                }

                if keep_cursor_at_end {
                    let cursor = CCursor::new(self.query.len());
                    let selection = CCursorRange::one(cursor);
                    if let Some(mut state) = TextEdit::load_state(ctx, text_edit_id) {
                        state.cursor.set_char_range(Some(selection));
                        state.store(ctx, text_edit_id);
                    }
                    ctx.request_repaint();
                }

                ui.add_space(8.0);

                let max_x = ui.available_rect_before_wrap().right();
                let mut current_x = ui.cursor().left();
                let font_id = FontId::monospace(self.font_size);
                let padding = 12.0;

                let mut visible_indices = Vec::new();
                let char_width = ctx.fonts_mut(|f| f.glyph_width(&font_id, 'M'));
                for (i, app) in results.iter().enumerate().skip(self.scroll_offset) {
                    let text_width = char_width * app.name.len() as f32 + padding;

                    if current_x + text_width > max_x && !visible_indices.is_empty() {
                        break;
                    }

                    visible_indices.push(i);
                    current_x += text_width;
                }

                for &i in &visible_indices {
                    let app = &results[i];
                    let is_selected = i == self.selected;
                    let (text_color, bg_color) = if is_selected {
                        (self.sel_fg, self.sel_bg)
                    } else {
                        (self.fg, self.bg)
                    };

                    let job = build_highlighted_text(
                        &app.name,
                        &app.match_indices,
                        text_color,
                        self.match_hl,
                        font_id.clone(),
                    );

                    let button_frame = egui::Frame::new()
                        .fill(bg_color)
                        .inner_margin(egui::Margin::symmetric(6, 0));

                    button_frame.show(ui, |ui| {
                        ui.label(job);
                    });
                }

                if let (Some(&first), Some(&last)) = (visible_indices.first(), visible_indices.last()) {
                    self.last_visible = last;
                    self.page_size = last - first + 1;
                }
            });
        });
    }
}
