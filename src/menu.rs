//! Menu interattivo TUI con logo a blocchi, navigazione freccia e bordo lampeggiante.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

// ---------------------------------------------------------------------------
// Colori
// ---------------------------------------------------------------------------
type Rgb = (u8, u8, u8);

const ACCENT: Rgb = (90, 224, 205);
const DIM: Rgb = (120, 130, 150);
const BRIGHT: Rgb = (255, 255, 255);
const BORDER_ON: Rgb = (220, 225, 240);
const BORDER_OFF: Rgb = (55, 60, 75);
const TITLE_DIM: Rgb = (80, 90, 110);
const WARN: Rgb = (245, 160, 80);

// ---------------------------------------------------------------------------
// Logo a blocchi — "PONG  ARENA"
// ---------------------------------------------------------------------------
const LOGO: [&str; 6] = [
    "██████╗  ██████╗ ███╗   ██╗ ██████╗     █████╗  ██████╗ ███████╗███╗   ██╗ █████╗",
    "██╔══██╗██╔═══██╗████╗  ██║██╔════╝    ██╔══██╗██╔════╝ ██╔════╝████╗  ██║██╔══██╗",
    "██████╔╝██║   ██║██╔██╗ ██║██║  ███╗   ███████║██║  ███╗█████╗  ██╔██╗ ██║███████║",
    "██╔═══╝ ██║   ██║██║╚██╗██║██║   ██║   ██╔══██║██║   ██║██╔══╝  ██║╚██╗██║██╔══██║",
    "██║     ╚██████╔╝██║ ╚████║╚██████╔╝   ██║  ██║╚██████╔╝███████╗██║ ╚████║██║  ██║",
    "╚═╝      ╚═════╝ ╚═╝  ╚═══╝ ╚═════╝    ╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═══╝╚═╝  ╚═╝",
];

// ---------------------------------------------------------------------------
// Tipi pubblici
// ---------------------------------------------------------------------------
pub enum MenuResult {
    Host,
    Join,
    Exit,
}

pub struct Config {
    pub nickname: String,
    pub port: u16,
    pub lives: i32,
    pub bots: usize,
    pub addr: String,
}

// ---------------------------------------------------------------------------
// ANSI helpers
// ---------------------------------------------------------------------------
fn fg(c: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", c.0, c.1, c.2)
}

fn mv(row: usize, col: usize) -> String {
    format!("\x1b[{};{}H", row + 1, col + 1)
}

fn centered_col(cols: usize, text: &str) -> usize {
    cols.saturating_sub(text.chars().count()) / 2
}

// ---------------------------------------------------------------------------
// Menu state
// ---------------------------------------------------------------------------
const ITEMS: &[&str] = &[
    "Host (multiplayer)",
    "Join (multiplayer)",
    "Nickname",
    "Porta",
    "Vite",
    "Bot",
    "Indirizzo IP",
    "Esci",
];

struct State {
    sel: usize,
    nickname: String,
    port: u16,
    lives: i32,
    bots: usize,
    addr: String,
    editing: Option<usize>,
    edit_buf: String,
    blink_on: bool,
    last_blink: Instant,
    error: Option<Instant>,
}

impl State {
    fn new() -> Self {
        State {
            sel: 0,
            nickname: String::new(),
            port: 7878,
            lives: 7,
            bots: 0,
            addr: String::new(),
            editing: None,
            edit_buf: String::new(),
            blink_on: true,
            last_blink: Instant::now(),
            error: None,
        }
    }

    fn num_items(&self) -> usize {
        ITEMS.len()
    }

    fn move_up(&mut self) {
        if self.editing.is_some() {
            return;
        }
        self.sel = if self.sel == 0 {
            self.num_items() - 1
        } else {
            self.sel - 1
        };
    }

    fn move_down(&mut self) {
        if self.editing.is_some() {
            return;
        }
        self.sel = (self.sel + 1) % self.num_items();
    }

    fn tick(&mut self) {
        if self.last_blink.elapsed() >= Duration::from_millis(400) {
            self.blink_on = !self.blink_on;
            self.last_blink = Instant::now();
        }
        if let Some(t) = self.error {
            if t.elapsed() > Duration::from_secs(3) {
                self.error = None;
            }
        }
    }

    fn display_value(&self, idx: usize) -> String {
        match idx {
            0 => ITEMS[0].to_string(),
            1 => ITEMS[1].to_string(),
            2 => format!(
                "Nickname:  {}",
                if self.nickname.is_empty() {
                    "(vuoto)"
                } else {
                    &self.nickname
                }
            ),
            3 => format!("Porta:     {}", self.port),
            4 => format!("Vite:      {}", self.lives),
            5 => format!("Bot:       {}", self.bots),
            6 => format!(
                "Indirizzo: {}",
                if self.addr.is_empty() {
                    "(vuoto)"
                } else {
                    &self.addr
                }
            ),
            7 => ITEMS[7].to_string(),
            _ => String::new(),
        }
    }

    fn start_edit(&mut self) {
        self.editing = Some(self.sel);
        self.edit_buf = match self.sel {
            2 => self.nickname.clone(),
            3 => self.port.to_string(),
            4 => self.lives.to_string(),
            5 => self.bots.to_string(),
            6 => self.addr.clone(),
            _ => String::new(),
        };
    }

    fn confirm_edit(&mut self) {
        if let Some(idx) = self.editing.take() {
            match idx {
                2 => self.nickname = self.edit_buf.clone(),
                3 => {
                    self.port = self.edit_buf.parse().unwrap_or(self.port);
                }
                4 => {
                    let v: i32 = self.edit_buf.parse().unwrap_or(self.lives);
                    if v >= 1 && v <= 99 {
                        self.lives = v;
                    } else {
                        self.error = Some(Instant::now());
                    }
                }
                5 => {
                    let v: usize = self.edit_buf.parse().unwrap_or(self.bots);
                    if v <= 7 {
                        self.bots = v;
                    } else {
                        self.error = Some(Instant::now());
                    }
                }
                6 => self.addr = self.edit_buf.clone(),
                _ => {}
            }
        }
    }

    fn cancel_edit(&mut self) {
        self.editing = None;
    }

    fn to_config(&self) -> Config {
        Config {
            nickname: self.nickname.clone(),
            port: self.port,
            lives: self.lives,
            bots: self.bots,
            addr: self.addr.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------
fn render(out: &mut String, s: &State, cols: usize, rows: usize) {
    out.push_str("\x1b[2J"); // clear screen
    out.push_str(&format!("\x1b[1J")); // clear above cursor too

    // Centra verticalmente: ~20 righe di contenuto
    let start = rows.saturating_sub(22) / 2;
    let mut row = start;

    // ── Logo ──
    for line in LOGO.iter() {
        let col = centered_col(cols, line);
        out.push_str(&mv(row, col));
        out.push_str(&fg(ACCENT));
        out.push_str(line);
        row += 1;
    }
    row += 1;

    // ── Sottotitolo ──
    {
        let sub = "Pong LAN adattivo";
        let col = centered_col(cols, sub);
        out.push_str(&mv(row, col));
        out.push_str(&fg(TITLE_DIM));
        out.push_str(sub);
    }
    row += 2;

    // ── Voci di menu ──
    let border = if s.blink_on { BORDER_ON } else { BORDER_OFF };

    for i in 0..s.num_items() {
        let text = s.display_value(i);

        if i == s.sel && s.editing != Some(i) {
            // ── Voce selezionata con bordo lampeggiante ──
            let inner_w = text.chars().count() + 6; // "  → " + text + "  "
            let box_w = inner_w + 2; // +2 per │ sinistra e destra
            let col = cols.saturating_sub(box_w) / 2;

            // Riga superiore
            out.push_str(&mv(row, col));
            out.push_str(&fg(border));
            out.push_str("┌");
            out.push_str(&"─".repeat(inner_w));
            out.push_str("┐");
            row += 1;

            // Contenuto
            out.push_str(&mv(row, col));
            out.push_str(&fg(border));
            out.push_str("│");
            out.push_str(&fg(BRIGHT));
            out.push_str(&format!("  → {} ", text));
            out.push_str(&fg(border));
            out.push_str("│");
            row += 1;

            // Riga inferiore
            out.push_str(&mv(row, col));
            out.push_str(&fg(border));
            out.push_str("└");
            out.push_str(&"─".repeat(inner_w));
            out.push_str("┘");
            row += 1;
        } else if i == s.sel && s.editing == Some(i) {
            // ── Modo editing ──
            let display = format!("{}: [{}]", ITEMS[i], s.edit_buf);
            let inner_w = display.chars().count() + 4;
            let box_w = inner_w + 2;
            let col = cols.saturating_sub(box_w) / 2;

            out.push_str(&mv(row, col));
            out.push_str(&fg(border));
            out.push_str("┌");
            out.push_str(&"─".repeat(inner_w));
            out.push_str("┐");
            row += 1;

            out.push_str(&mv(row, col));
            out.push_str(&fg(border));
            out.push_str("│ ");
            out.push_str(&fg(ACCENT));
            out.push_str(&display);
            out.push_str(&fg(border));
            out.push_str(" │");
            row += 1;

            out.push_str(&mv(row, col));
            out.push_str(&fg(border));
            out.push_str("└");
            out.push_str(&"─".repeat(inner_w));
            out.push_str("┘");
            row += 1;
        } else if i == 2 && s.editing != Some(2) {
            // ── Separatore ──
            let sep = "──────── Impostazioni ────────";
            let col = centered_col(cols, sep);
            out.push_str(&mv(row, col));
            out.push_str(&fg(DIM));
            out.push_str(sep);
            row += 1;
        } else {
            // ── Voce normale ──
            let col = centered_col(cols, &text);
            out.push_str(&mv(row, col));
            out.push_str(&fg(DIM));
            out.push_str(&text);
            row += 1;
        }
    }

    row += 1;

    // ── Errore ──
    if s.error.is_some() {
        let msg = "Valore non valido!";
        let col = centered_col(cols, msg);
        out.push_str(&mv(row, col));
        out.push_str(&fg(WARN));
        out.push_str(msg);
        row += 1;
    }

    row += 1;

    // ── Barra navigazione ──
    let nav = if s.editing.is_some() {
        "INVIO conferma   ESC annulla"
    } else {
        "↑/↓  muovi   INVIO  seleziona   Q  esci"
    };
    let col = centered_col(cols, nav);
    out.push_str(&mv(row, col));
    out.push_str(&fg(DIM));
    out.push_str(nav);

    // Reset
    out.push_str("\x1b[0m");
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------
pub fn run_menu() -> Option<(MenuResult, Config)> {
    let _guard = crate::terminal::TerminalGuard::new().ok()?;

    let mut s = State::new();

    loop {
        let (cw, rh) = crossterm::terminal::size().unwrap_or((80, 24));
        let cols = cw as usize;
        let rows = rh as usize;

        // Terminale troppo piccolo
        if cols < 72 || rows < 22 {
            let mut buf = String::from("\x1b[2J");
            let msg = "Ingrandisci il terminale (min ~72×22)";
            let col = cols.saturating_sub(msg.len()) / 2;
            buf.push_str(&mv(rows / 2, col));
            buf.push_str(&fg(WARN));
            buf.push_str(msg);
            buf.push_str("\x1b[0m");
            let _ = io::stdout().write_all(buf.as_bytes());
            let _ = io::stdout().flush();
            if event::poll(Duration::from_millis(100)).ok()? {
                if let Event::Key(k) = event::read().ok()? {
                    if matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) {
                        return None;
                    }
                }
            }
            continue;
        }

        s.tick();

        // Render
        let mut buf = String::new();
        render(&mut buf, &s, cols, rows);
        let _ = io::stdout().write_all(buf.as_bytes());
        let _ = io::stdout().flush();

        // Input
        if event::poll(Duration::from_millis(16)).ok()? {
            if let Event::Key(key) = event::read().ok()? {
                if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    continue;
                }

                // Ctrl+C sempre
                if key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    return None;
                }

                match key.code {
                    // ── Navigazione ──
                    KeyCode::Up | KeyCode::Char('k') => s.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => s.move_down(),

                    // ── Invio ──
                    KeyCode::Enter => {
                        if s.editing.is_some() {
                            s.confirm_edit();
                        } else {
                            match s.sel {
                                0 => return Some((MenuResult::Host, s.to_config())),
                                1 => return Some((MenuResult::Join, s.to_config())),
                                7 => return Some((MenuResult::Exit, s.to_config())),
                                _ => s.start_edit(),
                            }
                        }
                    }

                    // ── Esc / Q ──
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                        if s.editing.is_some() {
                            s.cancel_edit();
                        } else {
                            return Some((MenuResult::Exit, s.to_config()));
                        }
                    }

                    // ── Editing: carattere ──
                    KeyCode::Char(c)
                        if s.editing.is_some()
                            && !key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        s.edit_buf.push(c);
                    }

                    // ── Editing: backspace ──
                    KeyCode::Backspace if s.editing.is_some() => {
                        s.edit_buf.pop();
                    }

                    _ => {}
                }
            }
        }
    }
}
