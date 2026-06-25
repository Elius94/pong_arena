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
mod config;
mod game;
mod geom;
mod menu;
mod net;
mod render;
mod replay;
mod scores;
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
           pong_arena join <ip> [--port N] [--nickname NAME]\n  \
           pong_arena leaderboard                  mostra la classifica salvata\n\
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
           SPACE            spara   G  granata\n  \
           R                rivincita (a fine partita)   Q  esci\n\
         \n\
         ESEMPI:\n  \
           pong_arena host                 duello: aspetta 1 avversario\n  \
           pong_arena host --bots 3        arena a 4 lati tu + 3 bot\n  \
           pong_arena join 192.168.1.20    unisciti all'host\n  \
           pong_arena leaderboard          classifica globale"
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
// Main
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Se zero argomenti → menu interattivo TUI; torna al menu dopo ogni partita/replay.
    if args.is_empty() {
        loop {
            match menu::run_menu() {
                Some(menu::MenuResult::Host { port, bots, lives, nickname, avatar }) => {
                    let base = if nickname.is_empty() { "Host".to_string() } else { nickname };
                    let name = if avatar.is_empty() { base } else { format!("{} {}", avatar, base) };
                    if let Err(e) = app::run_host(port, bots, name, lives) {
                        eprintln!("Errore host: {e}");
                    }
                }
                Some(menu::MenuResult::Join { addr, port, nickname, avatar }) => {
                    let base = if nickname.is_empty() { "Guest".to_string() } else { nickname };
                    let name = if avatar.is_empty() { base } else { format!("{} {}", avatar, base) };
                    if let Err(e) = app::run_guest(&addr, port, name) {
                        eprintln!("Errore guest: {e}");
                    }
                }
                Some(menu::MenuResult::Replay { path }) => {
                    if let Err(e) = app::run_replay(std::path::Path::new(&path)) {
                        eprintln!("Errore replay: {e}");
                    }
                }
                Some(menu::MenuResult::Exit) | None => break,
            }
        }
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
            if bots > 39 {
                bots = 39;
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
        "leaderboard" | "classifica" | "scores" => {
            scores::print_leaderboard();
        }
        _ => usage(),
    }
}
