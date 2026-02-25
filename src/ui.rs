use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, Write};

use crate::config::{parse_hex_color, Config};
use crate::filter::FilteredApp;

pub struct Ui {
    config: Config,
    fg: Color,
    bg: Color,
    sel_fg: Color,
    sel_bg: Color,
    match_hl: Color,
    prompt_color: Color,
}

impl Ui {
    pub fn new(config: Config) -> Self {
        let fg = parse_hex_color(&config.appearance.foreground).unwrap_or(Color::White);
        let bg = parse_hex_color(&config.appearance.background).unwrap_or(Color::Black);
        let sel_fg = parse_hex_color(&config.appearance.selection_fg).unwrap_or(Color::White);
        let sel_bg = parse_hex_color(&config.appearance.selection_bg).unwrap_or(Color::Blue);
        let match_hl = parse_hex_color(&config.appearance.match_highlight).unwrap_or(Color::Green);
        let prompt_color = parse_hex_color(&config.appearance.prompt_color).unwrap_or(Color::Magenta);
        Self { config, fg, bg, sel_fg, sel_bg, match_hl, prompt_color }
    }

    pub fn enter(&self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;
        Ok(())
    }

    pub fn leave(&self) -> io::Result<()> {
        let mut stdout = io::stdout();
        execute!(stdout, Show, LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }

    pub fn render(&self, query: &str, results: &[FilteredApp], selected: usize) -> io::Result<()> {
        let mut stdout = io::stdout();
        let (cols, _) = terminal::size()?;
        execute!(stdout, MoveTo(0, 0))?;
        execute!(stdout, SetBackgroundColor(self.bg), SetForegroundColor(self.prompt_color))?;
        let input_line = format!("{}", query);
        let padding = " ".repeat((cols as usize).saturating_sub(input_line.len()));
        execute!(stdout, Print(&input_line), Print(&padding))?;
        for (i, app) in results.iter().enumerate() {
            execute!(stdout, MoveTo(0, (i + 1) as u16))?;
            let is_selected = i == selected;
            let (fg, bg) = if is_selected {
                (self.sel_fg, self.sel_bg)
            } else {
                (self.fg, self.bg)
            };
            execute!(stdout, SetBackgroundColor(bg), SetForegroundColor(fg))?;
            let mut line = String::new();
            for (ci, ch) in app.name.chars().enumerate() {
                if app.match_indices.contains(&ci) {
                    line.push_str(&format!(
                        "\x1b[38;2;{};{};{}m{}\x1b[38;2;{};{};{}m",
                        color_r(self.match_hl), color_g(self.match_hl), color_b(self.match_hl),
                        ch,
                        color_r(fg), color_g(fg), color_b(fg)
                    ));
                } else {
                    line.push(ch);
                }
            }
            let visible_len = app.name.len();
            let padding = " ".repeat((cols as usize).saturating_sub(visible_len));
            execute!(stdout, Print(&line), Print(&padding))?;
        }
        let total_lines = 1 + results.len();
        let max_lines = 1 + self.config.appearance.max_results;
        for i in total_lines..max_lines {
            execute!(stdout, MoveTo(0, i as u16))?;
            execute!(stdout, SetBackgroundColor(self.bg), SetForegroundColor(self.fg))?;
            let padding = " ".repeat(cols as usize);
            execute!(stdout, Print(&padding))?;
        }
        execute!(stdout, ResetColor)?;
        stdout.flush()?;
        Ok(())
    }

    pub fn max_results(&self) -> usize {
        self.config.appearance.max_results
    }
}

fn color_r(c: Color) -> u8 {
    match c {
        Color::Rgb { r, .. } => r,
        _ => 255,
    }
}

fn color_g(c: Color) -> u8 {
    match c {
        Color::Rgb { g, .. } => g,
        _ => 255,
    }
}

fn color_b(c: Color) -> u8 {
    match c {
        Color::Rgb { b, .. } => b,
        _ => 255,
    }
}

pub enum Action {
    Char(char),
    Backspace,
    Up,
    Down,
    Enter,
    Escape,
    None,
}

pub fn read_key() -> io::Result<Action> {
    loop {
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                return Ok(match code {
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => Action::Escape,
                    KeyCode::Char(c) => Action::Char(c),
                    KeyCode::Backspace => Action::Backspace,
                    KeyCode::Up => Action::Up,
                    KeyCode::Down => Action::Down,
                    KeyCode::Enter => Action::Enter,
                    KeyCode::Esc => Action::Escape,
                    _ => Action::None,
                });
            }
        }
    }
}
