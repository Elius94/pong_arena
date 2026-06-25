//! Menu interattivo TUI con logo a blocchi, navigazione freccia e bordo lampeggiante.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::config::{Config, AVATARS};
use crate::replay;
use crate::scores;

// ---------------------------------------------------------------------------
// Colori
// ---------------------------------------------------------------------------
type Rgb = (u8, u8, u8);

const ACCENT:     Rgb = (90,  224, 205);
const DIM:        Rgb = (120, 130, 150);
const BRIGHT:     Rgb = (255, 255, 255);
const BORDER_ON:  Rgb = (220, 225, 240);
const BORDER_OFF: Rgb = (55,  60,  75);
const TITLE_DIM:  Rgb = (80,  90,  110);
const WARN:       Rgb = (245, 160, 80);
const GOLD:       Rgb = (250, 196, 60);
const SILVER:     Rgb = (200, 200, 220);
const BRONZE:     Rgb = (180, 120, 60);

// ---------------------------------------------------------------------------
// Logo a blocchi
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
    Host   { port: u16, bots: usize, lives: i32, nickname: String, avatar: String },
    Join   { addr: String, port: u16, nickname: String, avatar: String },
    Replay { path: String },
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
#[derive(PartialEq)]
enum Screen {
    Main,
    HostConfig,
    JoinConfig,
    Discovery,
    Leaderboard,
    Replays,
}

struct State {
    screen: Screen,
    sel: usize,
    nickname: String,
    avatar_idx: usize,
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
        let cfg = Config::load();
        let avatar_idx = AVATARS.iter().position(|&a| a == cfg.avatar).unwrap_or(0);
        let (join_addr, join_port) = if cfg.last_server.is_empty() {
            (String::new(), 7878u16)
        } else if let Some(pos) = cfg.last_server.rfind(':') {
            let addr = cfg.last_server[..pos].to_string();
            let port = cfg.last_server[pos + 1..].parse().unwrap_or(7878);
            (addr, port)
        } else {
            (cfg.last_server.clone(), 7878)
        };
        State {
            screen: Screen::Main,
            sel: 0,
            nickname: cfg.nickname,
            avatar_idx,
            host_port: 7878,
            host_bots: 0,
            host_lives: 7,
            join_addr,
            join_port,
            editing: None,
            edit_buf: String::new(),
            blink_on: true,
            last_blink: Instant::now(),
            error: None,
        }
    }

    fn num_items(&self) -> usize {
        match self.screen {
            // 0=Host 1=Join 2=Scopri 3=Classifica 4=Replay 5=Nickname 6=Avatar 7=Esci
            Screen::Main       => 8,
            Screen::HostConfig => 5, // Porta, Bot, Vite, Avvia, Indietro
            Screen::JoinConfig => 4, // Indirizzo, Porta, Connetti, Indietro
            Screen::Discovery | Screen::Leaderboard | Screen::Replays => 1,
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
            if t.elapsed() > Duration::from_secs(3) { self.error = None; }
        }
    }

    fn display_value(&self, idx: usize) -> String {
        match self.screen {
            Screen::Main => match idx {
                0 => "Host (multiplayer)".to_string(),
                1 => "Join (multiplayer)".to_string(),
                2 => "Scopri server LAN".to_string(),
                3 => "Classifica".to_string(),
                4 => "Replay partite".to_string(),
                5 => format!("Nickname:  {}",
                        if self.nickname.is_empty() { "(vuoto)" } else { &self.nickname }),
                6 => format!("Avatar:    ◀ {} ▶", AVATARS[self.avatar_idx]),
                7 => "Esci".to_string(),
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
                0 => format!("Indirizzo: {}",
                        if self.join_addr.is_empty() { "(vuoto)" } else { &self.join_addr }),
                1 => format!("Porta:     {}", self.join_port),
                2 => "▶  Connetti!".to_string(),
                3 => "← Indietro".to_string(),
                _ => String::new(),
            },
            Screen::Discovery | Screen::Leaderboard | Screen::Replays => String::new(),
        }
    }

    fn is_action(&self, idx: usize) -> bool {
        match self.screen {
            // 5=Nickname è edit; tutto il resto è action
            Screen::Main       => matches!(idx, 0|1|2|3|4|6|7),
            Screen::HostConfig => matches!(idx, 3 | 4),
            Screen::JoinConfig => matches!(idx, 2 | 3),
            Screen::Discovery | Screen::Leaderboard | Screen::Replays => false,
        }
    }

    fn start_edit(&mut self) {
        self.editing = Some(self.sel);
        self.edit_buf = match self.screen {
            Screen::Main => match self.sel {
                5 => self.nickname.clone(),
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
            Screen::Discovery | Screen::Leaderboard | Screen::Replays => String::new(),
        };
    }

    fn confirm_edit(&mut self) {
        if let Some(idx) = self.editing.take() {
            match self.screen {
                Screen::Main => {
                    if idx == 5 {
                        self.nickname = self.edit_buf.trim().to_string();
                    }
                }
                Screen::HostConfig => match idx {
                    0 => { self.host_port = self.edit_buf.parse().unwrap_or(self.host_port); }
                    1 => {
                        let v: usize = self.edit_buf.parse().unwrap_or(self.host_bots);
                        if v <= 39 { self.host_bots = v; } else { self.error = Some(Instant::now()); }
                    }
                    2 => {
                        let v: i32 = self.edit_buf.parse().unwrap_or(self.host_lives);
                        if v >= 1 && v <= 99 { self.host_lives = v; } else { self.error = Some(Instant::now()); }
                    }
                    _ => {}
                },
                Screen::JoinConfig => match idx {
                    0 => { self.join_addr = self.edit_buf.trim().to_string(); }
                    1 => { self.join_port = self.edit_buf.parse().unwrap_or(self.join_port); }
                    _ => {}
                },
                Screen::Discovery | Screen::Leaderboard | Screen::Replays => {}
            }
        }
    }

    fn cancel_edit(&mut self) { self.editing = None; }

    fn avatar(&self) -> String { AVATARS[self.avatar_idx].to_string() }

    fn save_config(&self, update_server: bool) {
        let last_server = if update_server {
            format!("{}:{}", self.join_addr, self.join_port)
        } else if self.join_addr.is_empty() {
            String::new()
        } else {
            format!("{}:{}", self.join_addr, self.join_port)
        };
        Config { nickname: self.nickname.clone(), avatar: self.avatar(), last_server }.save();
    }

    fn select(&mut self) -> Option<MenuResult> {
        match self.screen {
            Screen::Main => match self.sel {
                6 => { // Avatar: Enter cicla avanti
                    self.avatar_idx = (self.avatar_idx + 1) % AVATARS.len();
                    None
                }
                7 => Some(MenuResult::Exit),
                _ => None,
            },
            Screen::HostConfig => match self.sel {
                3 => {
                    self.save_config(false);
                    Some(MenuResult::Host {
                        port: self.host_port, bots: self.host_bots, lives: self.host_lives,
                        nickname: self.nickname.clone(), avatar: self.avatar(),
                    })
                }
                4 => None,
                _ => None,
            },
            Screen::JoinConfig => match self.sel {
                2 => {
                    if self.join_addr.is_empty() {
                        self.error = Some(Instant::now());
                        None
                    } else {
                        self.save_config(true);
                        Some(MenuResult::Join {
                            addr: self.join_addr.clone(), port: self.join_port,
                            nickname: self.nickname.clone(), avatar: self.avatar(),
                        })
                    }
                }
                3 => None,
                _ => None,
            },
            Screen::Discovery | Screen::Leaderboard | Screen::Replays => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering delle schermate speciali
// ---------------------------------------------------------------------------
fn render_discovery(
    out: &mut String, cols: usize, mut row: usize,
    discovered: &[(String, u16, String)], disc_sel: usize, disc_active: bool,
) {
    row += 1;
    let (status, sc) = if disc_active {
        ("Ricerca in corso…", DIM)
    } else {
        ("Impossibile avviare la ricerca.", WARN)
    };
    out.push_str(&mv(row, centered_col(cols, status)));
    out.push_str(&fg(sc));
    out.push_str(status);
    row += 2;

    if discovered.is_empty() {
        let msg = "Nessun server trovato.";
        out.push_str(&mv(row, centered_col(cols, msg)));
        out.push_str(&fg(DIM));
        out.push_str(msg);
        row += 2;
    } else {
        for (i, (daddr, dport, info)) in discovered.iter().enumerate() {
            let text   = format!("{}:{}   {}", daddr, dport, info);
            let prefix = if i == disc_sel { "▸ " } else { "  " };
            let col_c  = if i == disc_sel { BRIGHT } else { DIM };
            let full   = format!("{}{}", prefix, text);
            out.push_str(&mv(row, centered_col(cols, &full)));
            out.push_str(&fg(col_c));
            out.push_str(&full);
            row += 1;
        }
        row += 1;
    }

    let back_full = format!("{}← Indietro", if disc_sel == discovered.len() { "▸ " } else { "  " });
    out.push_str(&mv(row, centered_col(cols, &back_full)));
    out.push_str(&fg(if disc_sel == discovered.len() { BRIGHT } else { DIM }));
    out.push_str(&back_full);
    row += 2;

    let nav = "↑/↓  muovi   INVIO  connetti   ESC  indietro";
    out.push_str(&mv(row, centered_col(cols, nav)));
    out.push_str(&fg(DIM));
    out.push_str(nav);
}

fn render_leaderboard(
    out: &mut String, cols: usize, mut row: usize,
    scores: &[scores::ScoreEntry],
) {
    row += 1;
    if scores.is_empty() {
        let msg = "Nessuna partita registrata ancora.";
        out.push_str(&mv(row, centered_col(cols, msg)));
        out.push_str(&fg(DIM));
        out.push_str(msg);
        row += 2;
    } else {
        let hdr = "  #   Nome                     Vittorie  Partite   Punti";
        out.push_str(&mv(row, centered_col(cols, hdr)));
        out.push_str(&fg(DIM));
        out.push_str(hdr);
        row += 1;
        let sep = "─".repeat(hdr.chars().count());
        out.push_str(&mv(row, centered_col(cols, &sep)));
        out.push_str(&fg(DIM));
        out.push_str(&sep);
        row += 1;
        for (i, e) in scores.iter().take(15).enumerate() {
            let col_c = match i { 0 => GOLD, 1 => SILVER, 2 => BRONZE, _ => DIM };
            let name: String = e.name.chars().take(22).collect();
            let line = format!("  {:>3}  {:<24}  {:>8}  {:>7}  {:>7}",
                i + 1, name, e.wins, e.games, e.points);
            out.push_str(&mv(row, centered_col(cols, &line)));
            out.push_str(&fg(col_c));
            out.push_str(&line);
            row += 1;
        }
        row += 1;
    }
    let nav = "ESC  indietro";
    out.push_str(&mv(row, centered_col(cols, nav)));
    out.push_str(&fg(DIM));
    out.push_str(nav);
}

fn render_replays(
    out: &mut String, cols: usize, mut row: usize,
    rep_list: &[replay::ReplayInfo], rep_sel: usize,
) {
    row += 1;
    if rep_list.is_empty() {
        let msg = "Nessun replay salvato.";
        out.push_str(&mv(row, centered_col(cols, msg)));
        out.push_str(&fg(DIM));
        out.push_str(msg);
        row += 2;
    } else {
        for (i, info) in rep_list.iter().enumerate() {
            let prefix = if i == rep_sel { "▸ " } else { "  " };
            let col_c  = if i == rep_sel { BRIGHT } else { DIM };
            let full   = format!("{}{}", prefix, info.display);
            out.push_str(&mv(row, centered_col(cols, &full)));
            out.push_str(&fg(col_c));
            out.push_str(&full);
            row += 1;
        }
        row += 1;
    }

    let back_sel  = rep_sel == rep_list.len();
    let back_full = format!("{}← Indietro", if back_sel { "▸ " } else { "  " });
    out.push_str(&mv(row, centered_col(cols, &back_full)));
    out.push_str(&fg(if back_sel { BRIGHT } else { DIM }));
    out.push_str(&back_full);
    row += 2;

    let nav = "↑/↓  muovi   INVIO  guarda   D  elimina   ESC  indietro";
    out.push_str(&mv(row, centered_col(cols, nav)));
    out.push_str(&fg(DIM));
    out.push_str(nav);
}

// ---------------------------------------------------------------------------
// Rendering principale (Main / HostConfig / JoinConfig)
// ---------------------------------------------------------------------------
fn render(
    out: &mut String,
    s: &State,
    cols: usize,
    rows: usize,
    discovered: &[(String, u16, String)],
    disc_sel: usize,
    disc_active: bool,
    scores: &[scores::ScoreEntry],
    rep_list: &[replay::ReplayInfo],
    rep_sel: usize,
) {
    out.push_str("\x1b[2J");
    out.push_str("\x1b[1J");

    let start = rows.saturating_sub(24) / 2;
    let mut row = start;

    for line in LOGO.iter() {
        let col = centered_col(cols, line);
        out.push_str(&mv(row, col));
        out.push_str(&fg(ACCENT));
        out.push_str(line);
        row += 1;
    }
    row += 1;

    let sub = match s.screen {
        Screen::Main        => "Pong LAN adattivo",
        Screen::HostConfig  => "Configurazione Host",
        Screen::JoinConfig  => "Configurazione Join",
        Screen::Discovery   => "Scoperta server LAN",
        Screen::Leaderboard => "Classifica Globale",
        Screen::Replays     => "Replay Partite",
    };
    let col = centered_col(cols, sub);
    out.push_str(&mv(row, col));
    out.push_str(&fg(TITLE_DIM));
    out.push_str(sub);
    row += 2;

    // Schermate speciali con rendering custom
    if matches!(s.screen, Screen::Discovery) {
        render_discovery(out, cols, row, discovered, disc_sel, disc_active);
        out.push_str("\x1b[0m");
        return;
    }
    if matches!(s.screen, Screen::Leaderboard) {
        render_leaderboard(out, cols, row, scores);
        out.push_str("\x1b[0m");
        return;
    }
    if matches!(s.screen, Screen::Replays) {
        render_replays(out, cols, row, rep_list, rep_sel);
        out.push_str("\x1b[0m");
        return;
    }

    // Menu standard
    let border = if s.blink_on { BORDER_ON } else { BORDER_OFF };

    for i in 0..s.num_items() {
        let text = s.display_value(i);

        if i == s.sel && s.editing == Some(i) {
            let display = format!("{}: [{}]", text.split(":  ").next().unwrap_or(&text), s.edit_buf);
            let inner_w = display.chars().count() + 4;
            let box_w   = inner_w + 2;
            let col     = cols.saturating_sub(box_w) / 2;
            out.push_str(&mv(row, col)); out.push_str(&fg(border));
            out.push_str("┌"); out.push_str(&"─".repeat(inner_w)); out.push_str("┐");
            row += 1;
            out.push_str(&mv(row, col)); out.push_str(&fg(border));
            out.push_str("│ "); out.push_str(&fg(ACCENT)); out.push_str(&display);
            out.push_str(&fg(border)); out.push_str(" │");
            row += 1;
            out.push_str(&mv(row, col)); out.push_str(&fg(border));
            out.push_str("└"); out.push_str(&"─".repeat(inner_w)); out.push_str("┘");
            row += 1;
        } else if i == s.sel {
            let inner_w = text.chars().count() + 6;
            let box_w   = inner_w + 2;
            let col     = cols.saturating_sub(box_w) / 2;
            out.push_str(&mv(row, col)); out.push_str(&fg(border));
            out.push_str("┌"); out.push_str(&"─".repeat(inner_w)); out.push_str("┐");
            row += 1;
            out.push_str(&mv(row, col)); out.push_str(&fg(border));
            out.push_str("│"); out.push_str(&fg(BRIGHT));
            out.push_str(&format!("  → {} ", text));
            out.push_str(&fg(border)); out.push_str("│");
            row += 1;
            out.push_str(&mv(row, col)); out.push_str(&fg(border));
            out.push_str("└"); out.push_str(&"─".repeat(inner_w)); out.push_str("┘");
            row += 1;
        } else {
            let col = centered_col(cols, &text);
            out.push_str(&mv(row, col));
            out.push_str(&fg(DIM));
            out.push_str(&text);
            row += 1;
        }
    }

    row += 1;
    if s.error.is_some() {
        let msg = "Valore non valido!";
        let col = centered_col(cols, msg);
        out.push_str(&mv(row, col)); out.push_str(&fg(WARN)); out.push_str(msg);
        row += 1;
    }
    row += 1;

    let nav = if s.editing.is_some() {
        "INVIO conferma   ESC annulla"
    } else {
        "↑/↓  muovi   INVIO  seleziona   ←/→  avatar   Q  esci"
    };
    let col = centered_col(cols, nav);
    out.push_str(&mv(row, col)); out.push_str(&fg(DIM)); out.push_str(nav);
    out.push_str("\x1b[0m");
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------
pub fn run_menu() -> Option<MenuResult> {
    let _guard = crate::terminal::TerminalGuard::new().ok()?;

    let mut s = State::new();

    // Stato scoperta LAN
    let mut disc_sock: Option<std::net::UdpSocket> = None;
    let mut discovered: Vec<(String, u16, String)> = Vec::new();
    let mut disc_sel: usize = 0;

    // Stato classifica (caricata all'ingresso della schermata)
    let mut scores_cache: Vec<scores::ScoreEntry> = Vec::new();

    // Stato replay
    let mut rep_list: Vec<replay::ReplayInfo> = Vec::new();
    let mut rep_sel: usize = 0;

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

        // ── Scoperta LAN ────────────────────────────────────────────────────
        if s.screen == Screen::Discovery {
            if disc_sock.is_none() {
                discovered.clear();
                disc_sel = 0;
                if let Ok(sock) = std::net::UdpSocket::bind(
                    ("0.0.0.0", crate::game::DISCOVERY_PORT)
                ) {
                    let _ = sock.set_nonblocking(true);
                    disc_sock = Some(sock);
                }
            }
            if let Some(ref sock) = disc_sock {
                let mut buf = [0u8; 256];
                loop {
                    match sock.recv_from(&mut buf) {
                        Ok((nb, src)) => {
                            let msg = std::str::from_utf8(&buf[..nb]).unwrap_or("").trim();
                            let parts: Vec<&str> = msg.split_whitespace().collect();
                            if parts.len() >= 4 && parts[0] == "PONG_ARENA" && parts[1] == "v1" {
                                if let Ok(p) = parts[2].parse::<u16>() {
                                    let ip = src.ip().to_string();
                                    let info = parts[3].to_string();
                                    if let Some(e) = discovered.iter_mut()
                                        .find(|(a, dp, _)| a == &ip && *dp == p)
                                    {
                                        e.2 = info;
                                    } else {
                                        discovered.push((ip, p, info));
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        } else {
            disc_sock = None;
        }
        let disc_active = disc_sock.is_some();

        // ── Render ──────────────────────────────────────────────────────────
        let mut buf = String::new();
        render(&mut buf, &s, cols, rows,
               &discovered, disc_sel, disc_active,
               &scores_cache, &rep_list, rep_sel);
        let _ = io::stdout().write_all(buf.as_bytes());
        let _ = io::stdout().flush();

        // ── Input ───────────────────────────────────────────────────────────
        if event::poll(Duration::from_millis(16)).ok()? {
            if let Event::Key(key) = event::read().ok()? {
                if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) { continue; }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return None;
                }

                // ── Elimina replay con D ─────────────────────────────────────
                if matches!(s.screen, Screen::Replays)
                    && matches!(key.code, KeyCode::Char('d') | KeyCode::Char('D'))
                {
                    if rep_sel < rep_list.len() {
                        let _ = replay::delete(&rep_list[rep_sel].path);
                        rep_list.remove(rep_sel);
                        if rep_sel > 0 && rep_sel >= rep_list.len() { rep_sel -= 1; }
                    }
                    continue;
                }

                match key.code {
                    // ── Navigazione ─────────────────────────────────────────
                    KeyCode::Up | KeyCode::Char('k') => {
                        match s.screen {
                            Screen::Discovery => { if disc_sel > 0 { disc_sel -= 1; } }
                            Screen::Replays   => { if rep_sel  > 0 { rep_sel  -= 1; } }
                            _ => s.move_up(),
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        match s.screen {
                            Screen::Discovery => {
                                let max = discovered.len();
                                if disc_sel < max { disc_sel += 1; }
                            }
                            Screen::Replays => {
                                let max = rep_list.len();
                                if rep_sel < max { rep_sel += 1; }
                            }
                            _ => s.move_down(),
                        }
                    }

                    KeyCode::Left => {
                        if s.editing.is_none() && s.screen == Screen::Main && s.sel == 6 {
                            s.avatar_idx = if s.avatar_idx == 0 { AVATARS.len() - 1 } else { s.avatar_idx - 1 };
                        }
                    }
                    KeyCode::Right => {
                        if s.editing.is_none() && s.screen == Screen::Main && s.sel == 6 {
                            s.avatar_idx = (s.avatar_idx + 1) % AVATARS.len();
                        }
                    }

                    // ── Conferma ─────────────────────────────────────────────
                    KeyCode::Enter => {
                        if s.screen == Screen::Discovery {
                            if disc_sel < discovered.len() {
                                let (daddr, dport, _) = discovered[disc_sel].clone();
                                s.join_addr = daddr;
                                s.join_port = dport;
                                s.screen = Screen::JoinConfig;
                                s.sel = 2;
                            } else {
                                s.screen = Screen::Main;
                                s.sel = 2;
                                disc_sock = None;
                            }
                        } else if s.screen == Screen::Replays {
                            if rep_sel < rep_list.len() {
                                let path = rep_list[rep_sel].path
                                    .to_string_lossy().to_string();
                                return Some(MenuResult::Replay { path });
                            } else {
                                s.screen = Screen::Main;
                                s.sel = 4;
                            }
                        } else if s.editing.is_some() {
                            s.confirm_edit();
                        } else if !s.is_action(s.sel) {
                            s.start_edit();
                        } else {
                            match s.sel {
                                0 if s.screen == Screen::Main => { s.screen = Screen::HostConfig; s.sel = 0; }
                                1 if s.screen == Screen::Main => { s.screen = Screen::JoinConfig; s.sel = 0; }
                                2 if s.screen == Screen::Main => {
                                    s.screen = Screen::Discovery; s.sel = 0; disc_sel = 0;
                                }
                                3 if s.screen == Screen::Main => {
                                    s.screen = Screen::Leaderboard; s.sel = 0;
                                    let mut sc = scores::load();
                                    sc.sort_by(|a, b| b.points.cmp(&a.points)
                                        .then(b.wins.cmp(&a.wins)).then(a.name.cmp(&b.name)));
                                    scores_cache = sc;
                                }
                                4 if s.screen == Screen::Main => {
                                    s.screen = Screen::Replays; s.sel = 0;
                                    rep_sel = 0;
                                    rep_list = replay::list();
                                }
                                4 if s.screen == Screen::HostConfig => {
                                    s.screen = Screen::Main; s.sel = 0;
                                }
                                3 if s.screen == Screen::JoinConfig => {
                                    s.screen = Screen::Main; s.sel = 1;
                                }
                                _ => {
                                    if let Some(result) = s.select() { return Some(result); }
                                }
                            }
                        }
                    }

                    // ── ESC / Q ───────────────────────────────────────────────
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                        if s.editing.is_some() {
                            s.cancel_edit();
                        } else {
                            match s.screen {
                                Screen::Main => return Some(MenuResult::Exit),
                                Screen::HostConfig | Screen::JoinConfig => {
                                    s.screen = Screen::Main; s.sel = 0;
                                }
                                Screen::Discovery => {
                                    s.screen = Screen::Main; s.sel = 2;
                                    disc_sock = None;
                                }
                                Screen::Leaderboard => { s.screen = Screen::Main; s.sel = 3; }
                                Screen::Replays     => { s.screen = Screen::Main; s.sel = 4; }
                            }
                        }
                    }

                    KeyCode::Char(c)
                        if s.editing.is_some()
                            && !key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        s.edit_buf.push(c);
                    }

                    KeyCode::Backspace if s.editing.is_some() => { s.edit_buf.pop(); }

                    _ => {}
                }
            }
        }
    }
}
