use std::collections::HashMap;
use std::fs;

use crate::config::{Config, Rgb, parse_hex_color};
use crate::frequency::Frequency;
use crate::filter::{filter_apps, FilteredApp};
use crate::launcher;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

use fontdue::{Font, FontSettings};
use xkbcommon::xkb;
use fontconfig::Fontconfig;

const WINDOW_HEIGHT: u16 = 28;

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
    xkb_state: xkb::State,
}

fn load_font(font_family: &str) -> Option<Font> {
    let fc = Fontconfig::new()?;
    let font_match = fc.find(font_family, None)?;
    let path = font_match.path;
    let data = fs::read(&path).ok()?;
    Font::from_bytes(data, FontSettings::default()).ok()
}

fn create_xkb_state() -> Result<xkb::State, Box<dyn std::error::Error>> {
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let keymap = xkb::Keymap::new_from_names(
        &context,
        "",
        "",
        "",
        "",
        None,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    ).ok_or("Failed to create XKB keymap")?;
    
    let state = xkb::State::new(&keymap);
    Ok(state)
}

impl App {
    fn new(config: Config, frequency: Frequency, apps: Vec<String>, xkb_state: xkb::State) -> Self {
        let font = load_font(&config.appearance.font_family)
            .unwrap_or_else(|| panic!("Font '{}' not found", config.appearance.font_family));
        
        let colors = CachedColors {
            bg: parse_hex_color(&config.appearance.background).unwrap_or(Rgb(33, 34, 44)),
            fg: parse_hex_color(&config.appearance.foreground).unwrap_or(Rgb(248, 248, 242)),
            sel_bg: parse_hex_color(&config.appearance.selection_bg).unwrap_or(Rgb(98, 114, 164)),
            sel_fg: parse_hex_color(&config.appearance.selection_fg).unwrap_or(Rgb(248, 248, 242)),
            match_hl: parse_hex_color(&config.appearance.match_highlight).unwrap_or(Rgb(139, 233, 253)),
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
            xkb_state,
        }
    }

    fn render(&mut self, width: u16, height: u16) -> Vec<u8> {
        let mut buffer = vec![0u8; width as usize * height as usize * 4];
        
        let bg = self.colors.bg;
        for pixel in buffer.chunks_exact_mut(4) {
            pixel[0] = bg.2; // B
            pixel[1] = bg.1; // G
            pixel[2] = bg.0; // R
            pixel[3] = 255;  // A
        }

        let dpi_scale = 96.0 / 72.0;
        let font_size = self.config.appearance.font_size as f32 * dpi_scale;
        let char_width = self.measure_text("M", font_size);
        let mut x_offset = 4;
        let y_offset = ((height as f32 - font_size) / 2.0) as i32 + font_size as i32;

        if let Some(ref name) = self.delete_confirm {
            let prompt = format!("Delete '{}'? (y/n)", name);
            self.draw_text(&mut buffer, width, &prompt, x_offset + char_width, y_offset, self.colors.prompt, &[], self.colors.prompt, font_size);
            return buffer;
        }

        let results = filter_apps(&self.apps, &self.query, &self.frequency);
        if self.selected >= results.len() {
            self.selected = results.len().saturating_sub(1);
        }

        let text_before_cursor: String = self.query.chars().take(self.cursor_pos).collect();
        let text_after_cursor: String = self.query.chars().skip(self.cursor_pos).collect();
        
        let before_width = self.draw_text(&mut buffer, width, &text_before_cursor, x_offset + char_width, y_offset, self.colors.prompt, &[], self.colors.prompt, font_size);
        
        let cursor_x = x_offset + char_width + before_width - 1;
        let cursor_height = (font_size * 1.2) as i32;
        let cursor_y = y_offset - font_size as i32 + 2;
        self.fill_rect(&mut buffer, width, cursor_x, cursor_y, 2, cursor_height, self.colors.prompt);
        
        let after_start = cursor_x + 3;
        self.draw_text(&mut buffer, width, &text_after_cursor, after_start, y_offset, self.colors.prompt, &[], self.colors.prompt, font_size);
        
        x_offset += (width as i32 / 4).saturating_sub(x_offset) + 8;

        let mut visible_count = 0;
        let mut first_visible_idx = None;
        
        for (i, app) in results.iter().enumerate().skip(self.scroll_offset) {
            if x_offset >= width as i32 {
                break;
            }

            let is_selected = i == self.selected;
            let (text_color, bg_color) = if is_selected {
                (self.colors.sel_fg, Some(self.colors.sel_bg))
            } else {
                (self.colors.fg, None)
            };

            let text_width = self.measure_text(&app.name, font_size) + 12;

            if let Some(bg) = bg_color {
                self.fill_rect(&mut buffer, width, x_offset, 0, text_width, height as i32, bg);
            }

            self.draw_text(&mut buffer, width, &app.name, x_offset + 6, y_offset, text_color, &app.match_indices, self.colors.match_hl, font_size);
            x_offset += text_width;
            
            if first_visible_idx.is_none() {
                first_visible_idx = Some(i);
            }
            self.last_visible = i;
            visible_count += 1;
        }
        
        self.page_size = visible_count;

        buffer
    }

    fn fill_rect(&self, buffer: &mut [u8], width: u16, x: i32, y: i32, w: i32, h: i32, color: Rgb) {
        for py in y.max(0)..(y + h).min(buffer.len() as i32 / (width as i32 * 4)) {
            for px in x.max(0)..(x + w).min(width as i32) {
                let idx = (py as usize * width as usize + px as usize) * 4;
                if idx + 3 < buffer.len() {
                    buffer[idx] = color.2;     // B
                    buffer[idx + 1] = color.1; // G
                    buffer[idx + 2] = color.0; // R
                    buffer[idx + 3] = 255;     // A
                }
            }
        }
    }

    fn draw_text(&mut self, buffer: &mut [u8], width: u16, text: &str, x: i32, y: i32, color: Rgb, match_indices: &[usize], highlight: Rgb, font_size: f32) -> i32 {
        let mut cursor_x = x;
        let px_size = font_size as u32;

        for (i, ch) in text.chars().enumerate() {
            let glyph_color = if match_indices.contains(&i) { highlight } else { color };
            
            let (metrics, bitmap) = self.glyph_cache
                .entry((ch, px_size))
                .or_insert_with(|| self.font.rasterize(ch, font_size));

            let gx = cursor_x + metrics.xmin;
            let gy = y - metrics.height as i32 - metrics.ymin;

            for py in 0..metrics.height {
                for px in 0..metrics.width {
                    let alpha = bitmap[py * metrics.width + px];
                    if alpha == 0 {
                        continue;
                    }
                    let dx = gx + px as i32;
                    let dy = gy + py as i32;
                    if dx < 0 || dy < 0 || dx >= width as i32 || dy >= (buffer.len() / (width as usize * 4)) as i32 {
                        continue;
                    }
                    let idx = (dy as usize * width as usize + dx as usize) * 4;
                    if idx + 3 >= buffer.len() {
                        continue;
                    }
                    let a = alpha as f32 / 255.0;
                    buffer[idx] = (glyph_color.2 as f32 * a + buffer[idx] as f32 * (1.0 - a)) as u8;
                    buffer[idx + 1] = (glyph_color.1 as f32 * a + buffer[idx + 1] as f32 * (1.0 - a)) as u8;
                    buffer[idx + 2] = (glyph_color.0 as f32 * a + buffer[idx + 2] as f32 * (1.0 - a)) as u8;
                }
            }

            cursor_x += metrics.advance_width as i32;
        }

        cursor_x - x
    }

    fn measure_text(&mut self, text: &str, font_size: f32) -> i32 {
        let px_size = font_size as u32;
        let mut width = 0;
        for ch in text.chars() {
            let (metrics, _) = self.glyph_cache
                .entry((ch, px_size))
                .or_insert_with(|| self.font.rasterize(ch, font_size));
            width += metrics.advance_width as i32;
        }
        width
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

    fn handle_key(&mut self, keycode: u8) -> Option<bool> {
        let results = filter_apps(&self.apps, &self.query, &self.frequency);
        let xkb_keycode = xkb::Keycode::new(keycode as u32);
        let keysym = self.xkb_state.key_get_one_sym(xkb_keycode);
        
        if self.delete_confirm.is_some() {
            match keysym {
                xkb::Keysym::y | xkb::Keysym::Y | xkb::Keysym::Return => {
                    if let Some(name) = self.delete_confirm.take() {
                        self.frequency.remove(&name);
                        let _ = self.frequency.save();
                    }
                }
                xkb::Keysym::n | xkb::Keysym::N | xkb::Keysym::Escape => {
                    self.delete_confirm = None;
                }
                _ => {}
            }
            return None;
        }

        let cursor_at_end = self.cursor_pos >= self.query.chars().count();
        
        match keysym {
            xkb::Keysym::Escape => Some(true),
            xkb::Keysym::Return | xkb::Keysym::KP_Enter => {
                self.launch_selected(&results);
                Some(true)
            }
            xkb::Keysym::Tab => {
                if let Some(app) = results.get(self.selected) {
                    self.query = app.name.clone();
                    self.cursor_pos = self.query.chars().count();
                    self.cursor_in_results = false;
                }
                None
            }
            xkb::Keysym::Delete => {
                if let Some(app) = results.get(self.selected) {
                    if self.frequency.get(&app.name) > 0 {
                        self.delete_confirm = Some(app.name.clone());
                    }
                }
                None
            }
            xkb::Keysym::BackSpace => {
                if self.cursor_pos > 0 {
                    let idx: usize = self.query.chars().take(self.cursor_pos - 1).map(|c| c.len_utf8()).sum();
                    let end_idx: usize = self.query.chars().take(self.cursor_pos).map(|c| c.len_utf8()).sum();
                    self.query.replace_range(idx..end_idx, "");
                    self.cursor_pos -= 1;
                    self.cursor_in_results = false;
                    self.selected = 0;
                    self.scroll_offset = 0;
                }
                None
            }
            xkb::Keysym::Left => {
                if self.cursor_in_results {
                    if self.selected > 0 {
                        if self.selected == self.scroll_offset && self.scroll_offset > 0 {
                            let prev_page_start = self.scroll_offset.saturating_sub(self.page_size.max(1));
                            self.scroll_offset = prev_page_start;
                        }
                        self.selected -= 1;
                    } else {
                        self.cursor_in_results = false;
                    }
                } else if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
                None
            }
            xkb::Keysym::Right => {
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
                let s = self.xkb_state.key_get_utf8(xkb_keycode);
                for ch in s.chars() {
                    if ch.is_control() {
                        continue;
                    }
                    let idx: usize = self.query.chars().take(self.cursor_pos).map(|c| c.len_utf8()).sum();
                    self.query.insert(idx, ch);
                    self.cursor_pos += 1;
                    self.cursor_in_results = false;
                    self.selected = 0;
                    self.scroll_offset = 0;
                }
                None
            }
        }
    }
    
    fn update_xkb_state(&mut self, keycode: u8, direction: xkb::KeyDirection) {
        let xkb_keycode = xkb::Keycode::new(keycode as u32);
        self.xkb_state.update_key(xkb_keycode, direction);
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
        self.conn.create_pixmap(self.depth, pixmap_id, self.win_id, self.current_width, self.current_height)?;
        
        self.conn.put_image(
            ImageFormat::Z_PIXMAP,
            pixmap_id,
            self.gc_id,
            self.current_width,
            self.current_height,
            0, 0,
            0,
            self.depth,
            pixels,
        )?;
        
        self.conn.copy_area(pixmap_id, self.win_id, self.gc_id, 0, 0, 0, 0, self.current_width, self.current_height)?;
        self.conn.free_pixmap(pixmap_id)?;
        self.conn.flush()?;
        Ok(())
    }
}

pub fn run(config: Config, frequency: Frequency, apps: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let width = screen.width_in_pixels;
    let root = screen.root;
    let depth = screen.root_depth;
    let visual = screen.root_visual;

    let win_id = conn.generate_id()?;
    let gc_id = conn.generate_id()?;

    conn.create_window(
        depth,
        win_id,
        root,
        0, 0,
        width, WINDOW_HEIGHT,
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
                    | EventMask::FOCUS_CHANGE
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
        5,    // flags: USPosition | PPosition
        0,    // x
        0,    // y
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
        &[hints.flags, hints.functions, hints.decorations, hints.input_mode as u32, hints.status],
    )?;

    let net_wm_window_type = conn.intern_atom(false, b"_NET_WM_WINDOW_TYPE")?.reply()?.atom;
    let net_wm_window_type_dock = conn.intern_atom(false, b"_NET_WM_WINDOW_TYPE_DOCK")?.reply()?.atom;
    conn.change_property32(
        PropMode::REPLACE,
        win_id,
        net_wm_window_type,
        AtomEnum::ATOM,
        &[net_wm_window_type_dock],
    )?;

    let net_wm_state = conn.intern_atom(false, b"_NET_WM_STATE")?.reply()?.atom;
    let net_wm_state_above = conn.intern_atom(false, b"_NET_WM_STATE_ABOVE")?.reply()?.atom;
    let net_wm_state_sticky = conn.intern_atom(false, b"_NET_WM_STATE_STICKY")?.reply()?.atom;
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

    let net_wm_strut_partial = conn.intern_atom(false, b"_NET_WM_STRUT_PARTIAL")?.reply()?.atom;
    conn.change_property32(
        PropMode::REPLACE,
        win_id,
        net_wm_strut_partial,
        AtomEnum::CARDINAL,
        &[0, 0, WINDOW_HEIGHT as u32, 0, 0, 0, 0, 0, 0, width as u32, 0, 0],
    )?;

    conn.map_window(win_id)?;
    conn.set_input_focus(InputFocus::PARENT, win_id, x11rb::CURRENT_TIME)?;
    conn.flush()?;

    let xkb_state = create_xkb_state()?;
    let mut app = App::new(config, frequency, apps, xkb_state);
    
    let mut ctx = X11Context {
        conn,
        win_id,
        gc_id,
        depth,
        current_width: width,
        current_height: WINDOW_HEIGHT,
    };

    loop {
        let event = ctx.conn.wait_for_event()?;
        
        match event {
            Event::ConfigureNotify(e) => {
                ctx.current_width = e.width;
                ctx.current_height = e.height;
            }
            Event::Expose(_) => {
                let pixels = app.render(ctx.current_width, ctx.current_height);
                ctx.redraw(&pixels)?;
            }
            Event::KeyPress(e) => {
                app.update_xkb_state(e.detail, xkb::KeyDirection::Down);
                
                if let Some(should_quit) = app.handle_key(e.detail) {
                    if should_quit {
                        break;
                    }
                }
                
                let pixels = app.render(ctx.current_width, ctx.current_height);
                ctx.redraw(&pixels)?;
            }
            Event::KeyRelease(e) => {
                app.update_xkb_state(e.detail, xkb::KeyDirection::Up);
            }
            _ => {}
        }
    }

    Ok(())
}
