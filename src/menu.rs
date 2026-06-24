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
// Logo a blocchi — "PONG  ARENA" (spaziatura corretta)
// ---------------------------------------------------------------------------
const LOGO: [&str; 6] = [
    "██████╗  ██████╗ ███╗   ██╗ ██████╗     █████╗ ██████╗  ███████╗███╗   ██╗ █████╗",
    "██╔══██╗██╔═══██╗████╗  ██║██╔════╝    ██╔══██╗██╔══██╗ ██╔════╝████╗  ██║██╔══██╗",
    "██████╔╝██║   ██║██╔██╗ ██║██║  ███╗   ███████║███████╔╝█████╗  ██╔██╗ ██║███████║",
    "██╔═══╝ ██║   ██║██║╚██╗██║██║   ██║   ██╔══██║██╔══██╗ ██╔══╝  ██║╚██╗██║██╔══██║",
    "██║     ╚██████╔╝██║ ╚████║╚██████╔╝   ██║  ██║██║  ╚██╗███████╗██║ ╚████║██║  ██║",
    "╚═╝      ╚═════╝ ╚═╝  ╚═══╝ ╚═════╝    ╚═╝  ╚═╝╚═╝  ╚═╝ ╚══════╝╚═╝  ╚═══╝╚═╝  ╚═╝",
];

// ---------------------------------------------------------------------------
// Tipi pubblici
// ---------------------------------------------------------------------------
pub enum MenuResult {
    Host { port: u16, bots: usize, lives: i32, nickname: String },
    Join { addr: String, port: u16, nickname: String },
    Exit,
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
// Stato menu principale
// ---------------------------------------------------------------------------
enum Screen {
    Main,
    HostConfig,
    JoinConfig,
}

struct State {
    screen: Screen,
    sel: usize,
    nickname: String,
    // host config
    host_port: u16,
    host_bots: usize,
    host_lives: i32,
    // join config
    join_addr: String,
    join_port: u16,
    // editing
    editing: Option<usize>,
    edit_buf: String,
    // blink
    blink_on: bool,
    last_blink: Instant,
    // error
    error: Option<Instant>,
}

impl State {
    fn new() -> Self {
        State {
            screen: Screen::Main,
            sel: 0,
            nickname: String::new(),
            host_port: 7878,
            host_bots: 0,
            host_lives: 7,
            join_addr: String::new(),
            join_port: 7878,
            editing: None,
            edit_buf: String::new(),
            blink_on: true,
            last_blink: Instant::now(),
            error: None,
        }
    }

    fn num_items(&self) -> usize {
        match self.screen {
            Screen::Main => 4,       // Host, Join, Nickname, Esci
            Screen::HostConfig => 5, // Porta, Bot, Vite, Avvia, Indietro
            Screen::JoinConfig => 4, // Indirizzo, Porta, Connetti, Indietro
        }
    }

    fn move_up(&mut self) {
        if self.editing.is_some() { return; }
        self.sel = if self.sel == 0 { self.num_items() - 1 } else { self.sel - 1 };
    }

    fn move_down(&mut self) {
        if self.editing.is_some() { return; }
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
        match self.screen {
            Screen::Main => match idx {
                0 => "Host (multiplayer)".to_string(),
                1 => "Join (multiplayer)".to_string(),
                2 => format!("Nickname:  {}", if self.nickname.is_empty() { "(vuoto)" } else { &self.nickname }),
                3 => "Esci".to_string(),
                _ => String::new(),
            },
            Screen::HostConfig => match idx {
                0 => format!("Porta:     {}", self.host_port),
                1 => format!("Bot:       {}", self.host_bots),
                2 => format!("Vite:      {}", self.host_lives),
                3 => "▶  Avvia!".to_string(),
                4 => "← Indietro".to_string(),
                _ => String::new(),
            },
            Screen::JoinConfig => match idx {
                0 => format!("Indirizzo: {}", if self.join_addr.is_empty() { "(vuoto)" } else { &self.join_addr }),
                1 => format!("Porta:     {}", self.join_port),
                2 => "▶  Connetti!".to_string(),
                3 => "← Indietro".to_string(),
                _ => String::new(),
            },
        }
    }

    fn is_action(&self, idx: usize) -> bool {
        match self.screen {
            Screen::Main => matches!(idx, 0 | 1 | 3),
            Screen::HostConfig => matches!(idx, 3 | 4),
            Screen::JoinConfig => matches!(idx, 2 | 3),
        }
    }

    fn start_edit(&mut self) {
        self.editing = Some(self.sel);
        self.edit_buf = match self.screen {
            Screen::Main => match self.sel {
                2 => self.nickname.clone(),
                _ => String::new(),
            },
            Screen::HostConfig => match self.sel {
                0 => self.host_port.to_string(),
                1 => self.host_bots.to_string(),
                2 => self.host_lives.to_string(),
                _ => String::new(),
            },
            Screen::JoinConfig => match self.sel {
                0 => self.join_addr.clone(),
                1 => self.join_port.to_string(),
                _ => String::new(),
            },
        };
    }

    fn confirm_edit(&mut self) {
        if let Some(idx) = self.editing.take() {
            match self.screen {
                Screen::Main => {
                    if idx == 2 { self.nickname = self.edit_buf.clone(); }
                }
                Screen::HostConfig => match idx {
                    0 => { self.host_port = self.edit_buf.parse().unwrap_or(self.host_port); }
                    1 => {
                        let v: usize = self.edit_buf.parse().unwrap_or(self.host_bots);
                        if v <= 19 { self.host_bots = v; } else { self.error = Some(Instant::now()); }
                    }
                    2 => {
                        let v: i32 = self.edit_buf.parse().unwrap_or(self.host_lives);
                        if v >= 1 && v <= 99 { self.host_lives = v; } else { self.error = Some(Instant::now()); }
                    }
                    _ => {}
                },
                Screen::JoinConfig => match idx {
                    0 => { self.join_addr = self.edit_buf.clone(); }
                    1 => { self.join_port = self.edit_buf.parse().unwrap_or(self.join_port); }
                    _ => {}
                },
            }
        }
    }

    fn cancel_edit(&mut self) {
        self.editing = None;
    }

    fn select(&mut self) -> Option<MenuResult> {
        match self.screen {
            Screen::Main => match self.sel {
                0 => {
                    // Host → vai al sottomenu host
                    None
                }
                1 => {
                    // Join → vai al sottomenu join
                    None
                }
                3 => Some(MenuResult::Exit),
                _ => None,
            },
            Screen::HostConfig => match self.sel {
                3 => Some(MenuResult::Host {
                    port: self.host_port,
                    bots: self.host_bots,
                    lives: self.host_lives,
                    nickname: self.nickname.clone(),
                }),
                4 => None, // back to main
                _ => None,
            },
            Screen::JoinConfig => match self.sel {
                2 => {
                    if self.join_addr.is_empty() {
                        self.error = Some(Instant::now());
                        None
                    } else {
                        Some(MenuResult::Join {
                            addr: self.join_addr.clone(),
                            port: self.join_port,
                            nickname: self.nickname.clone(),
                        })
                    }
                }
                3 => None, // back to main
                _ => None,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------
fn render(out: &mut String, s: &State, cols: usize, rows: usize) {
    out.push_str("\x1b[2J");
    out.push_str("\x1b[1J");

    let start = rows.saturating_sub(22) / 2;
    let mut row = start;

    // ── Logo (sempre visibile) ──
    for line in LOGO.iter() {
        let col = centered_col(cols, line);
        out.push_str(&mv(row, col));
        out.push_str(&fg(ACCENT));
        out.push_str(line);
        row += 1;
    }
    row += 1;

    // ── Sottotitolo ──
    let sub = match s.screen {
        Screen::Main => "Pong LAN adattivo",
        Screen::HostConfig => "Configurazione Host",
        Screen::JoinConfig => "Configurazione Join",
    };
    let col = centered_col(cols, sub);
    out.push_str(&mv(row, col));
    out.push_str(&fg(TITLE_DIM));
    out.push_str(sub);
    row += 2;

    // ── Voci di menu ──
    let border = if s.blink_on { BORDER_ON } else { BORDER_OFF };

    for i in 0..s.num_items() {

        let text = s.display_value(i);

        if i == s.sel && s.editing == Some(i) {
            // ── Modo editing ──
            let display = format!("{}: [{}]", text.split(":  ").next().unwrap_or(&text), s.edit_buf);
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

        } else if i == s.sel {
            // ── Voce selezionata con bordo lampeggiante ──
            let inner_w = text.chars().count() + 6;
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
            out.push_str("│");
            out.push_str(&fg(if s.is_action(i) { BRIGHT } else { BRIGHT }));
            out.push_str(&format!("  → {} ", text));
            out.push_str(&fg(border));
            out.push_str("│");
            row += 1;

            out.push_str(&mv(row, col));
            out.push_str(&fg(border));
            out.push_str("└");
            out.push_str(&"─".repeat(inner_w));
            out.push_str("┘");
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

    out.push_str("\x1b[0m");
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------
pub fn run_menu() -> Option<MenuResult> {
    let _guard = crate::terminal::TerminalGuard::new().ok()?;

    let mut s = State::new();

    loop {
        let (cw, rh) = crossterm::terminal::size().unwrap_or((80, 24));
        let cols = cw as usize;
        let rows = rh as usize;

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

        let mut buf = String::new();
        render(&mut buf, &s, cols, rows);
        let _ = io::stdout().write_all(buf.as_bytes());
        let _ = io::stdout().flush();

        if event::poll(Duration::from_millis(16)).ok()? {
            if let Event::Key(key) = event::read().ok()? {
                if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    continue;
                }

                if key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    return None;
                }

                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => s.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => s.move_down(),

                    KeyCode::Enter => {
                        if s.editing.is_some() {
                            s.confirm_edit();
                        } else if !s.is_action(s.sel) {
                            s.start_edit();
                        } else {
                            match s.sel {
                                // Menu principale
                                0 if matches!(s.screen, Screen::Main) => {
                                    s.screen = Screen::HostConfig;
                                    s.sel = 0;
                                }
                                1 if matches!(s.screen, Screen::Main) => {
                                    s.screen = Screen::JoinConfig;
                                    s.sel = 0;
                                }
                                // Host config: Indietro
                                4 if matches!(s.screen, Screen::HostConfig) => {
                                    s.screen = Screen::Main;
                                    s.sel = 0;
                                }
                                // Join config: Indietro
                                3 if matches!(s.screen, Screen::JoinConfig) => {
                                    s.screen = Screen::Main;
                                    s.sel = 1;
                                }
                                _ => {
                                    if let Some(result) = s.select() {
                                        return Some(result);
                                    }
                                }
                            }
                        }
                    }

                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                        if s.editing.is_some() {
                            s.cancel_edit();
                        } else {
                            match s.screen {
                                Screen::Main => return Some(MenuResult::Exit),
                                Screen::HostConfig | Screen::JoinConfig => {
                                    s.screen = Screen::Main;
                                    s.sel = 0;
                                }
                            }
                        }
                    }

                    KeyCode::Char(c)
                        if s.editing.is_some()
                            && !key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        s.edit_buf.push(c);
                    }

                    KeyCode::Backspace if s.editing.is_some() => {
                        s.edit_buf.pop();
                    }

                    _ => {}
                }
            }
        }
    }
}
