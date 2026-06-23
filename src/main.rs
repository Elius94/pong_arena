//! Pong Arena — Pong LAN adattivo.
//!
//! Con 2 giocatori è il Pong classico (campo rettangolare). Con 3 o più
//! giocatori il campo diventa un poligono regolare a N lati: ognuno difende un
//! lato e ha 7 vite ("schiaccia 7"). Chi le esaurisce viene eliminato e il suo
//! lato si chiude; vince l'ultimo rimasto.
//!
//! Uso:
//!   pong_arena                              menu interattivo
//!   pong_arena host [--port N] [--bots K] [--lives V] [--nickname NAME]
//!   pong_arena join <ip> [--port N] [--nickname NAME]

mod app;
mod arena;
mod game;
mod geom;
mod net;
mod render;
mod terminal;

use std::io::{self, Write};

const DEFAULT_PORT: u16 = 7878;
const DEFAULT_LIVES: i32 = 7;

fn usage() -> ! {
    eprintln!(
        "Pong Arena — Pong LAN adattivo (2 = classico, 3+ = arena poligonale)\n\
         \n\
         USO:\n  \
           pong_arena                              menu interattivo\n  \
           pong_arena host [--port N] [--bots K] [--lives V] [--nickname NAME]\n  \
           pong_arena join <ip> [--port N] [--nickname NAME]\n\
         \n\
         OPZIONI:\n  \
           --port N          porta TCP (default {DEFAULT_PORT})\n  \
           --bots K          riempi K posti con avversari IA (utile per provare da soli)\n  \
           --lives V         vite per giocatore (default {DEFAULT_LIVES})\n  \
           --nickname NAME   nickname del giocatore (se omesso viene chiesto)\n  \
           -h, --help        mostra questo aiuto\n\
         \n\
         COMANDI DI GIOCO:\n  \
           ←/→ · A/D · W/S  muovi la racchetta lungo il tuo lato\n  \
           R                rivincita (a fine partita)   Q  esci\n\
         \n\
         ESEMPI:\n  \
           pong_arena host                 duello: aspetta 1 avversario\n  \
           pong_arena host --bots 3        arena a 4 lati tu + 3 bot\n  \
           pong_arena join 192.168.1.20    unisciti all'host"
    );
    std::process::exit(2);
}

fn prompt_nickname() -> String {
    eprint!("Inserisci il tuo nickname: ");
    let _ = io::stdout().flush();
    let mut name = String::new();
    io::stdin().read_line(&mut name).unwrap_or(0);
    let name = name.trim().to_string();
    if name.is_empty() {
        "Guest".to_string()
    } else {
        name
    }
}

/// Legge una riga da stdin, trimma, e se vuota ritorna il default.
fn prompt_line(prompt: &str, default: &str) -> String {
    eprint!("{prompt} [{default}]: ");
    let _ = io::stdout().flush();
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).unwrap_or(0);
    let s = buf.trim().to_string();
    if s.is_empty() {
        default.to_string()
    } else {
        s
    }
}

/// Parse opzioni comuni (--port, --nickname) per il caso join.
fn parse_join_opts(args: &[String], i: &mut usize) -> (u16, String) {
    let mut port = DEFAULT_PORT;
    let mut nickname: Option<String> = None;
    let mut k = *i;
    while k < args.len() {
        match args[k].as_str() {
            "--port" => {
                k += 1;
                if let Some(p) = args.get(k).and_then(|v| v.parse().ok()) {
                    port = p;
                }
            }
            "--nickname" | "--name" => {
                k += 1;
                if let Some(n) = args.get(k) {
                    nickname = Some(n.clone());
                }
            }
            _ => break,
        }
        k += 1;
    }
    *i = k;
    let nick = nickname.unwrap_or_else(prompt_nickname);
    (port, nick)
}

// ---------------------------------------------------------------------------
// Menu interattivo
// ---------------------------------------------------------------------------

fn read_input() -> Option<String> {
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).ok()?;
    let s = buf.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn interactive_menu() {
    let mut nickname = String::new();
    let mut port = DEFAULT_PORT;
    let mut lives = DEFAULT_LIVES;

    loop {
        // Pulisci schermo (ANSI) per un menu pulito.
        eprint!("\x1b[2J\x1b[H");
        let _ = io::stderr().flush();

        let nick_display = if nickname.is_empty() {
            "(nessuno)".to_string()
        } else {
            nickname.clone()
        };

        eprintln!("╔════════════════════════════════╗");
        eprintln!("║        ▌ PONG · ARENA ▐        ║");
        eprintln!("╠════════════════════════════════╣");
        eprintln!("║                                ║");
        eprintln!("║  1  Host (multiplayer)         ║");
        eprintln!("║  2  Join (multiplayer)         ║");
        eprintln!("║                                ║");
        eprintln!("╠════════════════════════════════╣");
        eprintln!("║  3  Imposta nickname           ║");
        eprintln!("║  4  Imposta porta              ║");
        eprintln!("║  5  Imposta vite               ║");
        eprintln!("║                                ║");
        eprintln!("╠════════════════════════════════╣");
        eprintln!("║  Q  Esci                       ║");
        eprintln!("║                                ║");
        eprintln!("╚════════════════════════════════╝");
        eprintln!();
        eprintln!("  nickname: {nick_display}  porta: {port}  vite: {lives}");
        eprintln!();
        eprint!("Scegli: ");
        let _ = io::stderr().flush();

        let choice = match read_input() {
            Some(c) => c,
            None => continue,
        };

        match choice.as_str() {
            "1" | "h" | "H" => {
                // --- HOST ---
                let bots_s = prompt_line("Numero bot", "0");
                let bots: usize = bots_s.parse().unwrap_or(0).min(7);
                let name = if nickname.is_empty() {
                    prompt_nickname()
                } else {
                    nickname.clone()
                };
                eprintln!("Avvio host su porta {port} con {bots} bot, {lives} vite...");
                if let Err(e) = app::run_host(port, bots, name, lives) {
                    eprintln!("Errore host: {e}");
                    eprintln!("Premi INVIO per continuare...");
                    let _ = io::stdin().read_line(&mut String::new());
                }
            }
            "2" | "j" | "J" => {
                // --- JOIN ---
                let addr = prompt_line("Indirizzo IP", "127.0.0.1");
                let name = if nickname.is_empty() {
                    prompt_nickname()
                } else {
                    nickname.clone()
                };
                eprintln!("Connessione a {addr}:{port}...");
                if let Err(e) = app::run_guest(&addr, port, name) {
                    eprintln!("Errore guest: {e}");
                    eprintln!("Premi INVIO per continuare...");
                    let _ = io::stdin().read_line(&mut String::new());
                }
            }
            "3" | "n" | "N" => {
                // --- NICKNAME ---
                let new_nick = prompt_line("Nickname", &nickname);
                nickname = new_nick;
            }
            "4" | "p" | "P" => {
                // --- PORTA ---
                let port_s = prompt_line("Porta TCP", &port.to_string());
                if let Ok(p) = port_s.parse::<u16>() {
                    port = p;
                } else {
                    eprintln!("Porta non valida.");
                    eprintln!("Premi INVIO per continuare...");
                    let _ = io::stdin().read_line(&mut String::new());
                }
            }
            "5" | "v" | "V" => {
                // --- VITE ---
                let lives_s = prompt_line("Vite per giocatore", &lives.to_string());
                if let Ok(l) = lives_s.parse::<i32>() {
                    if l >= 1 && l <= 99 {
                        lives = l;
                    } else {
                        eprintln!("Valore fuori range (1-99).");
                        eprintln!("Premi INVIO per continuare...");
                        let _ = io::stdin().read_line(&mut String::new());
                    }
                } else {
                    eprintln!("Valore non valido.");
                    eprintln!("Premi INVIO per continuare...");
                    let _ = io::stdin().read_line(&mut String::new());
                }
            }
            "q" | "Q" | "x" | "X" => break,
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Se zero argomenti → menu interattivo.
    if args.is_empty() {
        interactive_menu();
        return;
    }

    if args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
    }

    match args[0].as_str() {
        "host" => {
            let mut port = DEFAULT_PORT;
            let mut bots = 0usize;
            let mut nickname: Option<String> = None;
            let mut lives = DEFAULT_LIVES;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--port" => {
                        i += 1;
                        port = match args.get(i).and_then(|v| v.parse().ok()) {
                            Some(p) => p,
                            None => usage(),
                        };
                    }
                    "--bots" => {
                        i += 1;
                        bots = match args.get(i).and_then(|v| v.parse().ok()) {
                            Some(b) => b,
                            None => usage(),
                        };
                    }
                    "--lives" => {
                        i += 1;
                        lives = match args.get(i).and_then(|v| v.parse().ok()) {
                            Some(l) => l,
                            None => usage(),
                        };
                    }
                    "--nickname" | "--name" => {
                        i += 1;
                        nickname = Some(match args.get(i) {
                            Some(n) => n.clone(),
                            None => usage(),
                        });
                    }
                    _ => usage(),
                }
                i += 1;
            }
            if bots > 7 {
                bots = 7;
            }
            let name = nickname.unwrap_or_else(|| "Host".to_string());
            if let Err(e) = app::run_host(port, bots, name, lives) {
                eprintln!("Errore host: {e}");
                std::process::exit(1);
            }
        }
        "join" => {
            if args.len() < 2 {
                usage();
            }
            let addr = args[1].clone();
            let mut i = 2;
            let (port, name) = parse_join_opts(&args, &mut i);
            if let Err(e) = app::run_guest(&addr, port, name) {
                eprintln!("Errore guest: {e}");
                std::process::exit(1);
            }
        }
        _ => usage(),
    }
}
