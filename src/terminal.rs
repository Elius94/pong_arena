//! Setup/teardown del terminale e lettura dell'input.
//!
//! L'input è espresso come "intento" di movimento: `+1` = su/destra, `-1` =
//! giù/sinistra, `0` = fermo. Il significato concreto (la racchetta scorre
//! lungo il proprio lato) è risolto lato simulazione in base alla vista del
//! giocatore, così i comandi risultano coerenti sia nel rettangolo a 2 sia nel
//! poligono.

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::{cursor, execute};
use std::io::{self, Write};
use std::time::{Duration, Instant};

fn restore() {
    let mut out = io::stdout();
    let _ = execute!(out, PopKeyboardEnhancementFlags);
    let _ = execute!(out, cursor::Show, LeaveAlternateScreen);
    let _ = disable_raw_mode();
    let _ = out.flush();
}

/// Guard RAII: predispone il terminale e lo ripristina in `drop`, anche in caso
/// di panic (grazie all'hook installato).
pub struct TerminalGuard;

impl TerminalGuard {
    pub fn new() -> io::Result<Self> {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            default_hook(info);
        }));

        enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen, cursor::Hide)?;

        let release_supported = supports_keyboard_enhancement().unwrap_or(false);
        if release_supported {
            let _ = execute!(
                out,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
            );
        }
        out.flush()?;
        Ok(TerminalGuard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// Stato dell'input locale.
pub struct InputState {
    pub intent: i32,
    pub quit: bool,
    pub restart: bool, // edge: vero per un frame, va consumato dal chiamante
    last_at: Instant,
    saw_release: bool,
}

/// Fallback (terminali senza eventi di rilascio): dopo questo tempo senza nuove
/// pressioni la racchetta si ferma.
const STILL_TIMEOUT: Duration = Duration::from_millis(160);

impl InputState {
    pub fn new() -> Self {
        InputState {
            intent: 0,
            quit: false,
            restart: false,
            last_at: Instant::now(),
            saw_release: false,
        }
    }

    pub fn pump(&mut self) -> io::Result<()> {
        while event::poll(Duration::from_secs(0))? {
            if let Event::Key(key) = event::read()? {
                self.handle_key(key);
            }
        }
        if !self.saw_release && self.intent != 0 && self.last_at.elapsed() > STILL_TIMEOUT {
            self.intent = 0;
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        let pos = matches!(
            key.code,
            KeyCode::Up
                | KeyCode::Right
                | KeyCode::Char('w')
                | KeyCode::Char('W')
                | KeyCode::Char('k')
                | KeyCode::Char('d')
                | KeyCode::Char('D')
                | KeyCode::Char('l')
        );
        let neg = matches!(
            key.code,
            KeyCode::Down
                | KeyCode::Left
                | KeyCode::Char('s')
                | KeyCode::Char('S')
                | KeyCode::Char('j')
                | KeyCode::Char('a')
                | KeyCode::Char('A')
                | KeyCode::Char('h')
        );

        match key.kind {
            KeyEventKind::Press | KeyEventKind::Repeat => {
                if pos {
                    self.intent = 1;
                    self.last_at = Instant::now();
                } else if neg {
                    self.intent = -1;
                    self.last_at = Instant::now();
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => self.quit = true,
                    KeyCode::Char('c') | KeyCode::Char('C')
                        if key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.quit = true
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => self.restart = true,
                    _ => {}
                }
            }
            KeyEventKind::Release => {
                self.saw_release = true;
                if (pos && self.intent == 1) || (neg && self.intent == -1) {
                    self.intent = 0;
                }
            }
        }
    }
}

/// Lettura semplice di un tasto durante la lobby/attesa: ritorna `Some(ch)` per
/// caratteri o tasti speciali, senza bloccare. Usa la coda eventi grezza.
pub enum Key {
    Enter,
    Space,
    Quit,
    Other,
}

pub fn poll_key() -> io::Result<Option<Key>> {
    if event::poll(Duration::from_secs(0))? {
        if let Event::Key(k) = event::read()? {
            if matches!(k.kind, KeyEventKind::Press) {
                let key = match k.code {
                    KeyCode::Enter => Key::Enter,
                    KeyCode::Char(' ') => Key::Space,
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => Key::Quit,
                    KeyCode::Char('c') | KeyCode::Char('C')
                        if k.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        Key::Quit
                    }
                    _ => Key::Other,
                };
                return Ok(Some(key));
            }
        }
    }
    Ok(None)
}
