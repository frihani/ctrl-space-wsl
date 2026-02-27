use std::collections::HashMap;
use std::fs;
use std::process::Command;

use crate::config::{parse_hex_color, Config, Rgb};
use crate::filter::{filter_apps, FilteredApp};
use crate::frequency::Frequency;
use crate::launcher;

use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as RandrConnectionExt;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

use fontdue::{Font, FontSettings};

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

fn resolve_font_path(font_family: &str) -> Option<String> {
    let cache_dir = crate::config::config_dir();
    let cache_file = cache_dir.join("font_cache");

    if let Ok(content) = fs::read_to_string(&cache_file) {
        for line in content.lines() {
            if let Some((family, path)) = line.split_once('\t') {
                if family == font_family && std::path::Path::new(path).exists() {
                    return Some(path.to_string());
                }
            }
        }
    }

    let output = Command::new("fc-match")
        .args([font_family, "--format=%{file}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim().to_string();
    if path.is_empty() {
        return None;
    }

    let _ = fs::create_dir_all(&cache_dir);
    let entry = format!("{}\t{}\n", font_family, path);
    let _ = fs::write(&cache_file, entry);

    Some(path)
}

fn load_font(font_family: &str) -> Option<Font> {
    let path = resolve_font_path(font_family)?;
    let data = fs::read(&path).ok()?;
    Font::from_bytes(data, FontSettings::default()).ok()
}

fn downscale(src: &[u8], src_width: u16, src_height: u16, scale: u16) -> Vec<u8> {
    let dst_width = src_width / scale;
    let dst_height = src_height / scale;
    let mut dst = vec![0u8; dst_width as usize * dst_height as usize * 4];
    let scale_sq = (scale * scale) as u32;

    for dy in 0..dst_height {
        for dx in 0..dst_width {
            let mut r: u32 = 0;
            let mut g: u32 = 0;
            let mut b: u32 = 0;

            for sy in 0..scale {
                for sx in 0..scale {
                    let src_x = dx * scale + sx;
                    let src_y = dy * scale + sy;
                    let src_idx = (src_y as usize * src_width as usize + src_x as usize) * 4;
                    b += src[src_idx] as u32;
                    g += src[src_idx + 1] as u32;
                    r += src[src_idx + 2] as u32;
                }
            }

            let dst_idx = (dy as usize * dst_width as usize + dx as usize) * 4;
            dst[dst_idx] = (b / scale_sq) as u8;
            dst[dst_idx + 1] = (g / scale_sq) as u8;
            dst[dst_idx + 2] = (r / scale_sq) as u8;
            dst[dst_idx + 3] = 255;
        }
    }

    dst
}

impl App {
    fn new(
        config: Config,
        frequency: Frequency,
        apps: Vec<String>,
        keymap: KeyboardMap,
        screen_width: u16,
        font: Font,
    ) -> Self {
        let colors = CachedColors {
            bg: parse_hex_color(&config.appearance.background).unwrap_or(Rgb(33, 34, 44)),
            fg: parse_hex_color(&config.appearance.foreground).unwrap_or(Rgb(248, 248, 242)),
            sel_bg: parse_hex_color(&config.appearance.selection_bg).unwrap_or(Rgb(98, 114, 164)),
            sel_fg: parse_hex_color(&config.appearance.selection_fg).unwrap_or(Rgb(248, 248, 242)),
            match_hl: parse_hex_color(&config.appearance.match_highlight)
                .unwrap_or(Rgb(139, 233, 253)),
            prompt: parse_hex_color(&config.appearance.prompt_color).unwrap_or(Rgb(189, 147, 249)),
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
        const SCALE: u16 = 2;
        let hi_width = width * SCALE;
        let hi_height = height * SCALE;
        let hi_buffer = self.render_internal(hi_width, hi_height, SCALE as f32);
        downscale(&hi_buffer, hi_width, hi_height, SCALE)
    }

    fn render_internal(&mut self, width: u16, height: u16, scale: f32) -> Vec<u8> {
        let mut buffer = vec![0u8; width as usize * height as usize * 4];

        let bg = self.colors.bg;
        for pixel in buffer.chunks_exact_mut(4) {
            pixel[0] = bg.2;
            pixel[1] = bg.1;
            pixel[2] = bg.0;
            pixel[3] = 255;
        }

        let dpi_scale = self.config.appearance.dpi as f32 / 72.0;
        let font_size = self.config.appearance.font_size as f32 * dpi_scale * scale;
        let scale_i = scale as i32;
        let baseline = (((height as f32 - font_size) / 2.0) + font_size - 2.0 * scale) as i32;
        let char_width = self.measure_text("M", font_size);
        let mut x_offset: i32 = 4 * scale_i;

        if let Some(ref name) = self.delete_confirm {
            let prompt = format!("Delete '{}'? (y/n)", name);
            self.draw_text(
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

        self.draw_text(
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
        let cursor_offset = self.measure_text(&text_before_cursor, font_size) - scale_i;
        let cursor_x = text_start + cursor_offset;
        let cursor_y = baseline - font_size as i32;
        let cursor_height = (font_size * 1.4) as i32;
        self.fill_rect(
            &mut buffer,
            width,
            cursor_x,
            cursor_y,
            2 * scale_i,
            cursor_height,
            cursor_color,
        );

        x_offset = ((width as i32 / 4) - x_offset).max(0) + x_offset + 8 * scale_i;

        let right_padding = char_width;
        let max_x = width as i32 - right_padding;

        let mut visible_count = 0;

        for (i, app) in results.iter().enumerate().skip(self.scroll_offset) {
            let text_width = self.measure_text(&app.name, font_size) + 14 * scale_i;

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
                self.fill_rect(
                    &mut buffer,
                    width,
                    x_offset,
                    0,
                    text_width,
                    height as i32,
                    bg_c,
                );
            }

            self.draw_text(
                &mut buffer,
                width,
                &app.name,
                x_offset + 6 * scale_i,
                baseline,
                text_color,
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
    fn fill_rect(
        &self,
        buffer: &mut [u8],
        buf_width: u16,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: Rgb,
    ) {
        let row_count = buffer.len() as i32 / (buf_width as i32 * 4);

        for py in y.max(0)..(y + h).min(row_count) {
            for px in x.max(0)..(x + w).min(buf_width as i32) {
                let idx = (py as usize * buf_width as usize + px as usize) * 4;
                buffer[idx] = color.2;
                buffer[idx + 1] = color.1;
                buffer[idx + 2] = color.0;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &mut self,
        buffer: &mut [u8],
        buf_width: u16,
        text: &str,
        x: i32,
        baseline: i32,
        color: Rgb,
        match_indices: &[usize],
        highlight: Rgb,
        font_size: f32,
    ) -> i32 {
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

            let gx = cursor_x + metrics.xmin;
            let gy = baseline - metrics.height as i32 - metrics.ymin;

            for py in 0..metrics.height {
                for px in 0..metrics.width {
                    let dx = gx + px as i32;
                    let dy = gy + py as i32;
                    if dx < 0 || dy < 0 || dx >= buf_width as i32 || dy >= row_count as i32 {
                        continue;
                    }

                    let alpha = bitmap[py * metrics.width + px] as f32 / 255.0;
                    if alpha == 0.0 {
                        continue;
                    }

                    let idx = (dy as usize * buf_width as usize + dx as usize) * 4;
                    let inv = 1.0 - alpha;
                    buffer[idx] = (glyph_color.2 as f32 * alpha + buffer[idx] as f32 * inv) as u8;
                    buffer[idx + 1] =
                        (glyph_color.1 as f32 * alpha + buffer[idx + 1] as f32 * inv) as u8;
                    buffer[idx + 2] =
                        (glyph_color.0 as f32 * alpha + buffer[idx + 2] as f32 * inv) as u8;
                }
            }

            cursor_x += metrics.advance_width.round() as i32;
        }

        cursor_x - x
    }

    fn measure_text(&mut self, text: &str, font_size: f32) -> i32 {
        let px_size = font_size as u32;
        let mut width: i32 = 0;
        for ch in text.chars() {
            let (metrics, _) = self
                .glyph_cache
                .entry((ch, px_size))
                .or_insert_with(|| self.font.rasterize(ch, font_size));
            width += metrics.advance_width.round() as i32;
        }
        width
    }

    fn find_page_containing(
        &mut self,
        results: &[FilteredApp],
        target_idx: usize,
        screen_width: u16,
    ) -> usize {
        let dpi_scale = self.config.appearance.dpi as f32 / 72.0;
        let font_size = self.config.appearance.font_size as f32 * dpi_scale;
        let char_width = self.measure_text("M", font_size);
        let x_start: i32 = 4;
        let results_start_x = ((screen_width as i32 / 4) - x_start).max(0) + x_start + 8;
        let right_padding = char_width;
        let max_x = screen_width as i32 - right_padding;
        let available_width = max_x - results_start_x;

        let mut page_start = 0;

        while page_start < results.len() {
            let mut x: i32 = 0;
            let mut page_end = page_start;

            for (i, result) in results.iter().enumerate().skip(page_start) {
                let item_width = self.measure_text(&result.name, font_size) + 12;
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

    fn launch_selected(&mut self, results: &[FilteredApp]) -> Option<bool> {
        let command = if let Some(app) = results.get(self.selected) {
            app.name.clone()
        } else if !self.query.trim().is_empty() {
            self.query.clone()
        } else {
            return Some(true);
        };

        let result = launcher::launch_command(&command);
        if result.success && !result.command.is_empty() {
            self.frequency.increment(&result.command);
            let _ = self.frequency.save();
        }
        if result.needs_delay {
            None // Signal to hide window and delay before exit
        } else {
            Some(true)
        }
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
            keysym::RETURN | keysym::KP_ENTER => match self.launch_selected(&results) {
                Some(quit) => Some(quit),
                None => Some(false), // Signal: hide, delay, then quit
            },
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

fn is_xwayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

fn compute_window_height(font: &Font, font_size: f32) -> u16 {
    // Upper padding of 2px and lower padding of 2px
    if let Some(metrics) = font.horizontal_line_metrics(font_size) {
        (metrics.ascent - metrics.descent).ceil() as u16 + 4
    } else {
        (font_size.ceil() as u16) + 4
    }
}

struct MonitorGeometry {
    x: i16,
    y: i16,
    width: u16,
}

/// Detect the monitor containing the currently focused window using RandR.
/// Falls back to full screen width at (0, 0) if anything fails.
fn get_active_monitor(conn: &impl Connection, root: Window, screen_width: u16) -> MonitorGeometry {
    let fallback = MonitorGeometry {
        x: 0,
        y: 0,
        width: screen_width,
    };

    // Get the currently focused window
    let focus = match conn.get_input_focus() {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => reply.focus,
            Err(_) => return fallback,
        },
        Err(_) => return fallback,
    };

    // Root or None means no usable focused window
    if focus == root || focus == 0 {
        return fallback;
    }

    // Translate the focused window origin to root coordinates
    let (focus_x, focus_y) = match conn.translate_coordinates(focus, root, 0, 0) {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => (reply.dst_x as i32, reply.dst_y as i32),
            Err(_) => return fallback,
        },
        Err(_) => return fallback,
    };

    // Query RandR for monitor/CRTC info
    let resources = match conn.randr_get_screen_resources_current(root) {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => reply,
            Err(_) => return fallback,
        },
        Err(_) => return fallback,
    };

    let mut best: Option<MonitorGeometry> = None;
    for &crtc in &resources.crtcs {
        let info = match conn.randr_get_crtc_info(crtc, 0) {
            Ok(cookie) => match cookie.reply() {
                Ok(info) => info,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        // Skip disabled outputs (width/height 0)
        if info.width == 0 || info.height == 0 {
            continue;
        }

        let cx = info.x as i32;
        let cy = info.y as i32;
        let cw = info.width as i32;
        let ch = info.height as i32;

        if focus_x >= cx && focus_x < cx + cw && focus_y >= cy && focus_y < cy + ch {
            return MonitorGeometry {
                x: info.x,
                y: info.y,
                width: info.width,
            };
        }

        // Track the first valid monitor as fallback
        if best.is_none() {
            best = Some(MonitorGeometry {
                x: info.x,
                y: info.y,
                width: info.width,
            });
        }
    }

    best.unwrap_or(fallback)
}

pub fn run(
    config: Config,
    frequency: Frequency,
    apps: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let setup = conn.setup();
    let screen = &setup.roots[screen_num];
    let root = screen.root;
    let depth = screen.root_depth;
    let visual = screen.root_visual;

    let monitor = get_active_monitor(&conn, root, screen.width_in_pixels);
    let mon_x = monitor.x;
    let mon_y = monitor.y;
    let width = monitor.width;

    let dpi_scale = config.appearance.dpi as f32 / 72.0;

    let font = load_font(&config.appearance.font_family)
        .unwrap_or_else(|| panic!("Font '{}' not found", config.appearance.font_family));
    let font_size = config.appearance.font_size as f32 * dpi_scale;
    let window_height = compute_window_height(&font, font_size);

    let keymap = KeyboardMap::new(&conn, setup)?;

    let win_id = conn.generate_id()?;
    let gc_id = conn.generate_id()?;

    let xwayland = is_xwayland();

    let mut win_aux = CreateWindowAux::new()
        .background_pixel(screen.black_pixel)
        .event_mask(
            EventMask::EXPOSURE
                | EventMask::KEY_PRESS
                | EventMask::KEY_RELEASE
                | EventMask::STRUCTURE_NOTIFY
                | EventMask::FOCUS_CHANGE,
        );

    if !xwayland {
        win_aux = win_aux.override_redirect(1);
    }

    conn.create_window(
        depth,
        win_id,
        root,
        mon_x,
        mon_y,
        width,
        window_height,
        0,
        WindowClass::INPUT_OUTPUT,
        visual,
        &win_aux,
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
        5,            // flags: USPosition | PPosition
        mon_x as u32, // x
        mon_y as u32, // y
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
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
        &[0, 0, (mon_y as u32) + (window_height as u32), 0],
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
            (mon_y as u32) + (window_height as u32),
            0,
            0,
            0,
            0,
            0,
            mon_x as u32,
            (mon_x as u32) + (width as u32) - 1,
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

    let mut app = App::new(config, frequency, apps, keymap, width, font);

    let mut ctx = X11Context {
        conn,
        win_id,
        gc_id,
        depth,
        current_width: width,
        current_height: window_height,
    };

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
                    } else {
                        // Hide window, delay for Windows exe, then exit
                        ctx.conn.unmap_window(ctx.win_id)?;
                        ctx.conn.flush()?;
                        std::thread::sleep(std::time::Duration::from_millis(500));
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
