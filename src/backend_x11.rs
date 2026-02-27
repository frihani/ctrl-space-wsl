use std::collections::HashMap;
use std::fs;
use std::process::Command;

use crate::config::{parse_hex_color, Config, Rgb};
use crate::filter::{filter_apps, FilteredApp};
use crate::frequency::Frequency;
use crate::launcher;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

use fontdue::{Font, FontSettings};

const WINDOW_HEIGHT: u16 = 30;

mod keysym {
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const RETURN: u32 = 0xff0d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const DELETE: u32 = 0xffff;
    pub const LEFT: u32 = 0xff51;
    pub const RIGHT: u32 = 0xff53;
    pub const KP_ENTER: u32 = 0xff8d;
}

struct KeyboardMap {
    keysyms: Vec<Keysym>,
    keysyms_per_keycode: u8,
    min_keycode: u8,
}

impl KeyboardMap {
    fn new<C: Connection>(conn: &C, setup: &Setup) -> Result<Self, Box<dyn std::error::Error>> {
        let min_keycode = setup.min_keycode;
        let max_keycode = setup.max_keycode;
        let count = max_keycode - min_keycode + 1;

        let reply = conn.get_keyboard_mapping(min_keycode, count)?.reply()?;

        Ok(Self {
            keysyms: reply.keysyms,
            keysyms_per_keycode: reply.keysyms_per_keycode,
            min_keycode,
        })
    }

    fn lookup(&self, keycode: u8, state: u16) -> Option<(u32, Option<char>)> {
        if keycode < self.min_keycode {
            return None;
        }

        let idx = (keycode - self.min_keycode) as usize * self.keysyms_per_keycode as usize;
        if idx >= self.keysyms.len() {
            return None;
        }

        let shift = (state & u16::from(KeyButMask::SHIFT)) != 0;
        let caps = (state & u16::from(KeyButMask::LOCK)) != 0;

        let base_keysym = self.keysyms.get(idx).copied().unwrap_or(0);
        let shifted_keysym = self.keysyms.get(idx + 1).copied().unwrap_or(0);

        if base_keysym == 0 {
            return None;
        }

        let is_letter =
            (0x61..=0x7a).contains(&base_keysym) || (0x41..=0x5a).contains(&base_keysym);

        let (keysym, ch) = if is_letter {
            let apply_upper = shift ^ caps;
            let ch = keysym_to_char(base_keysym, apply_upper);
            (base_keysym, ch)
        } else if shift && shifted_keysym != 0 {
            let ch = keysym_to_char(shifted_keysym, false);
            (shifted_keysym, ch)
        } else {
            let ch = keysym_to_char(base_keysym, false);
            (base_keysym, ch)
        };

        Some((keysym, ch))
    }
}

fn keysym_to_char(keysym: u32, apply_shift: bool) -> Option<char> {
    if (0x20..=0x7e).contains(&keysym) {
        let ch = keysym as u8 as char;
        if apply_shift {
            return Some(if ch.is_ascii_lowercase() {
                ch.to_ascii_uppercase()
            } else {
                ch
            });
        }
        return Some(ch);
    }

    if (0x0a0..=0x0ff).contains(&keysym) {
        return char::from_u32(keysym);
    }

    None
}

struct CachedColors {
    bg: Rgb,
    fg: Rgb,
    sel_bg: Rgb,
    sel_fg: Rgb,
    match_hl: Rgb,
    prompt: Rgb,
    args: Rgb,
}

struct App {
    config: Config,
    frequency: Frequency,
    apps: Vec<String>,
    query: String,
    cursor_pos: usize,
    selected: usize,
    scroll_offset: usize,
    last_visible: usize,
    page_size: usize,
    cursor_in_results: bool,
    delete_confirm: Option<String>,
    font: Font,
    glyph_cache: HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>,
    colors: CachedColors,
    keymap: KeyboardMap,
    screen_width: u16,
}

fn load_font(font_family: &str) -> Option<Font> {
    let output = Command::new("fc-match")
        .args([font_family, "--format=%{file}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let data = fs::read(path).ok()?;
    Font::from_bytes(data, FontSettings::default()).ok()
}

impl App {
    fn new(
        config: Config,
        frequency: Frequency,
        apps: Vec<String>,
        keymap: KeyboardMap,
        screen_width: u16,
    ) -> Self {
        let font = load_font(&config.appearance.font_family)
            .unwrap_or_else(|| panic!("Font '{}' not found", config.appearance.font_family));

        let colors = CachedColors {
            bg: parse_hex_color(&config.appearance.background).unwrap_or(Rgb(33, 34, 44)),
            fg: parse_hex_color(&config.appearance.foreground).unwrap_or(Rgb(248, 248, 242)),
            sel_bg: parse_hex_color(&config.appearance.selection_bg).unwrap_or(Rgb(98, 114, 164)),
            sel_fg: parse_hex_color(&config.appearance.selection_fg).unwrap_or(Rgb(248, 248, 242)),
            match_hl: parse_hex_color(&config.appearance.match_highlight)
                .unwrap_or(Rgb(139, 233, 253)),
            prompt: parse_hex_color(&config.appearance.prompt_color).unwrap_or(Rgb(189, 147, 249)),
            args: parse_hex_color(&config.appearance.args_color).unwrap_or(Rgb(255, 121, 198)),
        };

        Self {
            config,
            frequency,
            apps,
            query: String::new(),
            cursor_pos: 0,
            selected: 0,
            scroll_offset: 0,
            last_visible: 0,
            page_size: 0,
            cursor_in_results: false,
            delete_confirm: None,
            font,
            glyph_cache: HashMap::new(),
            colors,
            keymap,
            screen_width,
        }
    }

    fn render(&mut self, width: u16, height: u16) -> Vec<u8> {
        let mut buffer = vec![0u8; width as usize * height as usize * 4];

        let bg = self.colors.bg;
        for pixel in buffer.chunks_exact_mut(4) {
            pixel[0] = bg.2;
            pixel[1] = bg.1;
            pixel[2] = bg.0;
            pixel[3] = 255;
        }

        let dpi_scale: f32 = 96.0 / 72.0;
        let font_size = self.config.appearance.font_size as f32 * dpi_scale;
        let height_f = height as f32;
        let baseline = ((height_f - font_size) / 2.0) + font_size - 2.0;
        let char_width = self.measure_text_aa("M", font_size);
        let mut x_offset: f32 = 4.0;

        if let Some(ref name) = self.delete_confirm {
            let prompt = format!("Delete '{}'? (y/n)", name);
            self.draw_text_aa(
                &mut buffer,
                width,
                &prompt,
                x_offset + char_width,
                baseline,
                self.colors.prompt,
                &[],
                self.colors.prompt,
                font_size,
            );
            return buffer;
        }

        let results = filter_apps(&self.apps, &self.query, &self.frequency);
        if self.selected >= results.len() {
            self.selected = results.len().saturating_sub(1);
        }

        let text_start = x_offset + char_width;
        let query = self.query.clone();
        let text_before_cursor: String = query.chars().take(self.cursor_pos).collect();

        self.draw_text_aa(
            &mut buffer,
            width,
            &query,
            text_start,
            baseline,
            self.colors.prompt,
            &[],
            self.colors.prompt,
            font_size,
        );

        let cursor_color = Rgb(192, 222, 255);
        let cursor_offset = self.measure_text_aa(&text_before_cursor, font_size) - 1.0;
        let cursor_x = text_start + cursor_offset;
        let cursor_y = baseline - font_size;
        let cursor_height = font_size * 1.4;
        self.fill_rect_aa(
            &mut buffer,
            width,
            cursor_x,
            cursor_y,
            2.0,
            cursor_height,
            cursor_color,
        );

        x_offset = ((width as f32 / 4.0) - x_offset).max(0.0) + x_offset + 8.0;

        let right_padding = char_width;
        let max_x = width as f32 - right_padding;

        let mut visible_count = 0;

        for (i, app) in results.iter().enumerate().skip(self.scroll_offset) {
            let text_width = self.measure_text_aa(&app.name, font_size) + 14.0;

            if x_offset + text_width > max_x && visible_count > 0 {
                break;
            }

            let is_selected = i == self.selected;
            let (text_color, bg_color) = if is_selected {
                (self.colors.sel_fg, Some(self.colors.sel_bg))
            } else {
                (self.colors.fg, None)
            };

            if let Some(bg_c) = bg_color {
                self.fill_rect_aa(
                    &mut buffer,
                    width,
                    x_offset,
                    0.0,
                    text_width,
                    height_f,
                    bg_c,
                );
            }

            let args_start = app.name.find(' ').unwrap_or(app.name.len());
            self.draw_text_with_args(
                &mut buffer,
                width,
                &app.name,
                x_offset + 6.0,
                baseline,
                text_color,
                self.colors.args,
                args_start,
                &app.match_indices,
                self.colors.match_hl,
                font_size,
            );
            x_offset += text_width;

            self.last_visible = i;
            visible_count += 1;
        }

        self.page_size = visible_count;

        buffer
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_rect_aa(
        &self,
        buffer: &mut [u8],
        buf_width: u16,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Rgb,
    ) {
        let x_end = x + w;
        let y_end = y + h;

        let px_start = x.floor() as i32;
        let px_end = x_end.ceil() as i32;
        let py_start = y.floor() as i32;
        let py_end = y_end.ceil() as i32;

        let row_count = buffer.len() as i32 / (buf_width as i32 * 4);

        for py in py_start.max(0)..py_end.min(row_count) {
            let y_cov = (y_end.min((py + 1) as f32) - y.max(py as f32)).clamp(0.0, 1.0);

            for px in px_start.max(0)..px_end.min(buf_width as i32) {
                let x_cov = (x_end.min((px + 1) as f32) - x.max(px as f32)).clamp(0.0, 1.0);

                let alpha = x_cov * y_cov;
                if alpha <= 0.0 {
                    continue;
                }

                let idx = (py as usize * buf_width as usize + px as usize) * 4;
                if idx + 3 >= buffer.len() {
                    continue;
                }

                let inv = 1.0 - alpha;
                buffer[idx] = (color.2 as f32 * alpha + buffer[idx] as f32 * inv) as u8;
                buffer[idx + 1] = (color.1 as f32 * alpha + buffer[idx + 1] as f32 * inv) as u8;
                buffer[idx + 2] = (color.0 as f32 * alpha + buffer[idx + 2] as f32 * inv) as u8;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_aa(
        &mut self,
        buffer: &mut [u8],
        buf_width: u16,
        text: &str,
        x: f32,
        baseline: f32,
        color: Rgb,
        match_indices: &[usize],
        highlight: Rgb,
        font_size: f32,
    ) -> f32 {
        let mut cursor_x = x;
        let px_size = font_size as u32;
        let row_count = buffer.len() / (buf_width as usize * 4);

        for (i, ch) in text.chars().enumerate() {
            let glyph_color = if match_indices.contains(&i) {
                highlight
            } else {
                color
            };

            let (metrics, bitmap) = self
                .glyph_cache
                .entry((ch, px_size))
                .or_insert_with(|| self.font.rasterize(ch, font_size));

            let metrics = *metrics;
            let bitmap = bitmap.clone();

            let gx = (cursor_x + metrics.xmin as f32).round() as i32;
            let gy = (baseline - metrics.height as f32 - metrics.ymin as f32).round() as i32;

            for py in 0..metrics.height {
                for px in 0..metrics.width {
                    let alpha = bitmap[py * metrics.width + px];
                    if alpha == 0 {
                        continue;
                    }
                    let dx = gx + px as i32;
                    let dy = gy + py as i32;
                    if dx < 0 || dy < 0 || dx >= buf_width as i32 || dy >= row_count as i32 {
                        continue;
                    }
                    let idx = (dy as usize * buf_width as usize + dx as usize) * 4;
                    if idx + 3 >= buffer.len() {
                        continue;
                    }
                    let a = alpha as f32 / 255.0;
                    let inv = 1.0 - a;
                    buffer[idx] = (glyph_color.2 as f32 * a + buffer[idx] as f32 * inv) as u8;
                    buffer[idx + 1] =
                        (glyph_color.1 as f32 * a + buffer[idx + 1] as f32 * inv) as u8;
                    buffer[idx + 2] =
                        (glyph_color.0 as f32 * a + buffer[idx + 2] as f32 * inv) as u8;
                }
            }

            cursor_x += metrics.advance_width;
        }

        cursor_x - x
    }

    fn measure_text_aa(&mut self, text: &str, font_size: f32) -> f32 {
        let px_size = font_size as u32;
        let mut width: f32 = 0.0;
        for ch in text.chars() {
            let (metrics, _) = self
                .glyph_cache
                .entry((ch, px_size))
                .or_insert_with(|| self.font.rasterize(ch, font_size));
            width += metrics.advance_width;
        }
        width
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_with_args(
        &mut self,
        buffer: &mut [u8],
        buf_width: u16,
        text: &str,
        x: f32,
        baseline: f32,
        cmd_color: Rgb,
        args_color: Rgb,
        args_start_byte: usize,
        match_indices: &[usize],
        highlight: Rgb,
        font_size: f32,
    ) -> f32 {
        let mut cursor_x = x;
        let px_size = font_size as u32;
        let row_count = buffer.len() / (buf_width as usize * 4);

        let mut byte_pos = 0;
        for (i, ch) in text.chars().enumerate() {
            let in_args = byte_pos >= args_start_byte;
            let base_color = if in_args { args_color } else { cmd_color };
            let glyph_color = if match_indices.contains(&i) {
                highlight
            } else {
                base_color
            };

            let (metrics, bitmap) = self
                .glyph_cache
                .entry((ch, px_size))
                .or_insert_with(|| self.font.rasterize(ch, font_size));

            let metrics = *metrics;
            let bitmap = bitmap.clone();

            let gx = (cursor_x + metrics.xmin as f32).round() as i32;
            let gy = (baseline - metrics.height as f32 - metrics.ymin as f32).round() as i32;

            for py in 0..metrics.height {
                for px in 0..metrics.width {
                    let alpha = bitmap[py * metrics.width + px];
                    if alpha == 0 {
                        continue;
                    }
                    let dx = gx + px as i32;
                    let dy = gy + py as i32;
                    if dx < 0 || dy < 0 || dx >= buf_width as i32 || dy >= row_count as i32 {
                        continue;
                    }
                    let idx = (dy as usize * buf_width as usize + dx as usize) * 4;
                    if idx + 3 >= buffer.len() {
                        continue;
                    }
                    let a = alpha as f32 / 255.0;
                    let inv = 1.0 - a;
                    buffer[idx] = (glyph_color.2 as f32 * a + buffer[idx] as f32 * inv) as u8;
                    buffer[idx + 1] =
                        (glyph_color.1 as f32 * a + buffer[idx + 1] as f32 * inv) as u8;
                    buffer[idx + 2] =
                        (glyph_color.0 as f32 * a + buffer[idx + 2] as f32 * inv) as u8;
                }
            }

            cursor_x += metrics.advance_width;
            byte_pos += ch.len_utf8();
        }

        cursor_x - x
    }

    fn find_page_containing(
        &mut self,
        results: &[FilteredApp],
        target_idx: usize,
        screen_width: u16,
    ) -> usize {
        let dpi_scale: f32 = 96.0 / 72.0;
        let font_size = self.config.appearance.font_size as f32 * dpi_scale;
        let char_width = self.measure_text_aa("M", font_size);
        let x_start: f32 = 4.0;
        let results_start_x = ((screen_width as f32 / 4.0) - x_start).max(0.0) + x_start + 8.0;
        let right_padding = char_width;
        let max_x = screen_width as f32 - right_padding;
        let available_width = max_x - results_start_x;

        let mut page_start = 0;

        while page_start < results.len() {
            let mut x: f32 = 0.0;
            let mut page_end = page_start;

            for (i, result) in results.iter().enumerate().skip(page_start) {
                let item_width = self.measure_text_aa(&result.name, font_size) + 12.0;
                if x + item_width > available_width {
                    break;
                }
                x += item_width;
                page_end = i;
            }

            if target_idx >= page_start && target_idx <= page_end {
                return page_start;
            }

            page_start = page_end + 1;
        }

        0
    }

    fn launch_selected(&mut self, results: &[FilteredApp]) -> bool {
        let command = if let Some(app) = results.get(self.selected) {
            app.name.clone()
        } else if !self.query.trim().is_empty() {
            self.query.clone()
        } else {
            return true;
        };

        let result = launcher::launch_command(&command);
        if result.success && !result.command.is_empty() {
            self.frequency.increment(&result.command);
            let _ = self.frequency.save();
        }
        true
    }

    fn handle_key(&mut self, keycode: u8, state: u16) -> Option<bool> {
        let results = filter_apps(&self.apps, &self.query, &self.frequency);

        let (keysym, ch) = self.keymap.lookup(keycode, state)?;

        if self.delete_confirm.is_some() {
            match ch {
                Some('y') | Some('Y') => {
                    if let Some(name) = self.delete_confirm.take() {
                        self.frequency.remove(&name);
                        let _ = self.frequency.save();
                    }
                }
                Some('n') | Some('N') => {
                    self.delete_confirm = None;
                }
                _ if keysym == keysym::RETURN => {
                    if let Some(name) = self.delete_confirm.take() {
                        self.frequency.remove(&name);
                        let _ = self.frequency.save();
                    }
                }
                _ if keysym == keysym::ESCAPE => {
                    self.delete_confirm = None;
                }
                _ => {}
            }
            return None;
        }

        let cursor_at_end = self.cursor_pos >= self.query.chars().count();

        match keysym {
            keysym::ESCAPE => Some(true),
            keysym::RETURN | keysym::KP_ENTER => {
                self.launch_selected(&results);
                Some(true)
            }
            keysym::TAB => {
                if let Some(app) = results.get(self.selected) {
                    self.query = app.name.clone();
                    self.cursor_pos = self.query.chars().count();
                    self.cursor_in_results = false;
                }
                None
            }
            keysym::DELETE => {
                if let Some(app) = results.get(self.selected) {
                    if self.frequency.get(&app.name) > 0 {
                        self.delete_confirm = Some(app.name.clone());
                    }
                }
                None
            }
            keysym::BACKSPACE => {
                if self.cursor_pos > 0 {
                    let idx: usize = self
                        .query
                        .chars()
                        .take(self.cursor_pos - 1)
                        .map(|c| c.len_utf8())
                        .sum();
                    let end_idx: usize = self
                        .query
                        .chars()
                        .take(self.cursor_pos)
                        .map(|c| c.len_utf8())
                        .sum();
                    self.query.replace_range(idx..end_idx, "");
                    self.cursor_pos -= 1;
                    self.cursor_in_results = false;
                    self.selected = 0;
                    self.scroll_offset = 0;
                }
                None
            }
            keysym::LEFT => {
                if self.cursor_in_results {
                    if self.selected > 1 {
                        let new_selected = self.selected - 1;
                        if new_selected < self.scroll_offset {
                            self.scroll_offset = self.find_page_containing(
                                &results,
                                new_selected,
                                self.screen_width,
                            );
                        }
                        self.selected = new_selected;
                    } else {
                        self.cursor_in_results = false;
                        self.selected = 0;
                        self.scroll_offset = 0;
                    }
                } else if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
                None
            }
            keysym::RIGHT => {
                if self.cursor_in_results {
                    if self.selected < self.last_visible {
                        self.selected += 1;
                    } else if self.last_visible + 1 < results.len() {
                        self.scroll_offset = self.last_visible + 1;
                        self.selected = self.scroll_offset;
                    }
                } else if cursor_at_end && results.len() > 1 {
                    self.cursor_in_results = true;
                    self.selected = 1;
                } else if self.cursor_pos < self.query.chars().count() {
                    self.cursor_pos += 1;
                }
                None
            }
            _ => {
                if let Some(c) = ch {
                    if !c.is_control() {
                        let idx: usize = self
                            .query
                            .chars()
                            .take(self.cursor_pos)
                            .map(|c| c.len_utf8())
                            .sum();
                        self.query.insert(idx, c);
                        self.cursor_pos += 1;
                        self.cursor_in_results = false;
                        self.selected = 0;
                        self.scroll_offset = 0;
                    }
                }
                None
            }
        }
    }
}

struct X11Context {
    conn: x11rb::rust_connection::RustConnection,
    win_id: u32,
    gc_id: u32,
    depth: u8,
    current_width: u16,
    current_height: u16,
}

impl X11Context {
    fn redraw(&self, pixels: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let pixmap_id = self.conn.generate_id()?;
        self.conn.create_pixmap(
            self.depth,
            pixmap_id,
            self.win_id,
            self.current_width,
            self.current_height,
        )?;

        self.conn.put_image(
            ImageFormat::Z_PIXMAP,
            pixmap_id,
            self.gc_id,
            self.current_width,
            self.current_height,
            0,
            0,
            0,
            self.depth,
            pixels,
        )?;

        self.conn.copy_area(
            pixmap_id,
            self.win_id,
            self.gc_id,
            0,
            0,
            0,
            0,
            self.current_width,
            self.current_height,
        )?;
        self.conn.free_pixmap(pixmap_id)?;
        self.conn.flush()?;
        Ok(())
    }
}

pub fn run(
    config: Config,
    frequency: Frequency,
    apps: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let setup = conn.setup();
    let screen = &setup.roots[screen_num];
    let width = screen.width_in_pixels;
    let root = screen.root;
    let depth = screen.root_depth;
    let visual = screen.root_visual;

    let keymap = KeyboardMap::new(&conn, setup)?;

    let win_id = conn.generate_id()?;
    let gc_id = conn.generate_id()?;

    conn.create_window(
        depth,
        win_id,
        root,
        0,
        0,
        width,
        WINDOW_HEIGHT,
        0,
        WindowClass::INPUT_OUTPUT,
        visual,
        &CreateWindowAux::new()
            .background_pixel(screen.black_pixel)
            .event_mask(
                EventMask::EXPOSURE
                    | EventMask::KEY_PRESS
                    | EventMask::KEY_RELEASE
                    | EventMask::STRUCTURE_NOTIFY
                    | EventMask::FOCUS_CHANGE,
            ),
    )?;

    conn.create_gc(gc_id, win_id, &CreateGCAux::new())?;

    conn.change_property8(
        PropMode::REPLACE,
        win_id,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        b"ctrl-space-wsl",
    )?;

    let size_hints: [u32; 18] = [
        5, // flags: USPosition | PPosition
        0, // x
        0, // y
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    conn.change_property32(
        PropMode::REPLACE,
        win_id,
        AtomEnum::WM_NORMAL_HINTS,
        AtomEnum::WM_SIZE_HINTS,
        &size_hints,
    )?;

    let motif_wm_hints = conn.intern_atom(false, b"_MOTIF_WM_HINTS")?.reply()?.atom;
    #[repr(C)]
    struct MotifHints {
        flags: u32,
        functions: u32,
        decorations: u32,
        input_mode: i32,
        status: u32,
    }
    let hints = MotifHints {
        flags: 2,
        functions: 0,
        decorations: 0,
        input_mode: 0,
        status: 0,
    };
    conn.change_property32(
        PropMode::REPLACE,
        win_id,
        motif_wm_hints,
        motif_wm_hints,
        &[
            hints.flags,
            hints.functions,
            hints.decorations,
            hints.input_mode as u32,
            hints.status,
        ],
    )?;

    let net_wm_window_type = conn
        .intern_atom(false, b"_NET_WM_WINDOW_TYPE")?
        .reply()?
        .atom;
    let net_wm_window_type_dock = conn
        .intern_atom(false, b"_NET_WM_WINDOW_TYPE_DOCK")?
        .reply()?
        .atom;
    conn.change_property32(
        PropMode::REPLACE,
        win_id,
        net_wm_window_type,
        AtomEnum::ATOM,
        &[net_wm_window_type_dock],
    )?;

    let net_wm_state = conn.intern_atom(false, b"_NET_WM_STATE")?.reply()?.atom;
    let net_wm_state_above = conn
        .intern_atom(false, b"_NET_WM_STATE_ABOVE")?
        .reply()?
        .atom;
    let net_wm_state_sticky = conn
        .intern_atom(false, b"_NET_WM_STATE_STICKY")?
        .reply()?
        .atom;
    conn.change_property32(
        PropMode::REPLACE,
        win_id,
        net_wm_state,
        AtomEnum::ATOM,
        &[net_wm_state_above, net_wm_state_sticky],
    )?;

    let net_wm_strut = conn.intern_atom(false, b"_NET_WM_STRUT")?.reply()?.atom;
    conn.change_property32(
        PropMode::REPLACE,
        win_id,
        net_wm_strut,
        AtomEnum::CARDINAL,
        &[0, 0, WINDOW_HEIGHT as u32, 0],
    )?;

    let net_wm_strut_partial = conn
        .intern_atom(false, b"_NET_WM_STRUT_PARTIAL")?
        .reply()?
        .atom;
    conn.change_property32(
        PropMode::REPLACE,
        win_id,
        net_wm_strut_partial,
        AtomEnum::CARDINAL,
        &[
            0,
            0,
            WINDOW_HEIGHT as u32,
            0,
            0,
            0,
            0,
            0,
            0,
            width as u32,
            0,
            0,
        ],
    )?;

    let wm_hints: [u32; 9] = [
        1, // flags: InputHint
        1, // input: True
        0, 0, 0, 0, 0, 0, 0,
    ];
    conn.change_property32(
        PropMode::REPLACE,
        win_id,
        AtomEnum::WM_HINTS,
        AtomEnum::WM_HINTS,
        &wm_hints,
    )?;

    conn.map_window(win_id)?;
    conn.flush()?;

    let mut grabbed = false;
    for attempt in 0..100 {
        let reply = conn
            .grab_keyboard(
                false,
                win_id,
                x11rb::CURRENT_TIME,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .reply()?;
        if reply.status == GrabStatus::SUCCESS {
            grabbed = true;
            break;
        }
        if attempt == 99 {
            eprintln!(
                "warn: keyboard grab failed after 100 attempts (status {:?})",
                reply.status
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    if !grabbed {
        let _ = conn.set_input_focus(InputFocus::PARENT, win_id, x11rb::CURRENT_TIME);
        conn.flush()?;
    }

    let mut app = App::new(config, frequency, apps, keymap, width);

    let mut ctx = X11Context {
        conn,
        win_id,
        gc_id,
        depth,
        current_width: width,
        current_height: WINDOW_HEIGHT,
    };

    // Try a first render
    // let pixels = app.render(ctx.current_width, ctx.current_height);
    // ctx.redraw(&pixels)?;

    loop {
        let event = ctx.conn.wait_for_event()?;

        match event {
            Event::ConfigureNotify(e) => {
                ctx.current_width = e.width;
                ctx.current_height = e.height;
            }
            Event::MapNotify(_) => {
                ctx.conn.grab_keyboard(
                    true,
                    ctx.win_id,
                    x11rb::CURRENT_TIME,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )?;
                ctx.conn.flush()?;
            }
            Event::Expose(_) => {
                let pixels = app.render(ctx.current_width, ctx.current_height);
                ctx.redraw(&pixels)?;
            }
            Event::KeyPress(e) => {
                if let Some(should_quit) = app.handle_key(e.detail, e.state.into()) {
                    if should_quit {
                        break;
                    }
                }

                let pixels = app.render(ctx.current_width, ctx.current_height);
                ctx.redraw(&pixels)?;
            }
            Event::FocusOut(_) => {
                for _ in 0..50 {
                    let reply = ctx
                        .conn
                        .grab_keyboard(
                            false,
                            ctx.win_id,
                            x11rb::CURRENT_TIME,
                            GrabMode::ASYNC,
                            GrabMode::ASYNC,
                        )?
                        .reply()?;
                    if reply.status == GrabStatus::SUCCESS {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            _ => {}
        }
    }

    ctx.conn.ungrab_keyboard(x11rb::CURRENT_TIME)?;
    ctx.conn.flush()?;

    Ok(())
}
