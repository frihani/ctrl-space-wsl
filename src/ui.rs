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
        Self { fg, bg, sel_fg, sel_bg, match_hl, prompt_color }
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

    pub fn render(&self, query: &str, results: &[FilteredApp], selected: usize, scroll_offset: usize) -> io::Result<usize> {
        let mut stdout = io::stdout();
        let (cols, _) = terminal::size()?;
        let cols = cols as usize;
        let prompt_width = cols / 3;
        let suggestions_start = prompt_width;
        execute!(stdout, MoveTo(0, 0))?;
        execute!(stdout, SetBackgroundColor(self.bg))?;
        execute!(stdout, SetForegroundColor(self.prompt_color))?;
        let padding = " ";
        execute!(stdout, Print(padding))?;
        let available_prompt_width = prompt_width.saturating_sub(2);
        let display_query = if query.len() > available_prompt_width {
            &query[query.len() - available_prompt_width..]
        } else {
            query
        };
        execute!(stdout, Print(display_query))?;
        let prompt_padding = " ".repeat(available_prompt_width.saturating_sub(display_query.len()) + 1);
        execute!(stdout, Print(&prompt_padding))?;
        let mut x = suggestions_start;
        let padding = " ";
        let mut last_visible = scroll_offset;
        for (i, app) in results.iter().enumerate().skip(scroll_offset) {
            let entry_len = app.name.len() + 2;
            if x + entry_len > cols {
                break;
            }
            last_visible = i;
            let is_selected = i == selected;
            let (fg, bg) = if is_selected {
                (self.sel_fg, self.sel_bg)
            } else {
                (self.fg, self.bg)
            };
            execute!(stdout, SetBackgroundColor(bg), SetForegroundColor(fg))?;
            execute!(stdout, Print(padding))?;
            for (ci, ch) in app.name.chars().enumerate() {
                if app.match_indices.contains(&ci) {
                    execute!(stdout, SetForegroundColor(self.match_hl))?;
                    execute!(stdout, Print(ch))?;
                    execute!(stdout, SetForegroundColor(fg))?;
                } else {
                    execute!(stdout, Print(ch))?;
                }
            }
            execute!(stdout, Print(padding))?;
            x += app.name.len() + 2;
        }
        if x < cols {
            execute!(stdout, SetBackgroundColor(self.bg), SetForegroundColor(self.fg))?;
            let padding = " ".repeat(cols - x);
            execute!(stdout, Print(&padding))?;
        }
        execute!(stdout, ResetColor)?;
        stdout.flush()?;
        Ok(last_visible)
    }
}

pub enum Action {
    Char(char),
    Backspace,
    Left,
    Right,
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
                    KeyCode::Left => Action::Left,
                    KeyCode::Right => Action::Right,
                    KeyCode::Enter => Action::Enter,
                    KeyCode::Esc => Action::Escape,
                    _ => Action::None,
                });
            }
        }
    }
}
