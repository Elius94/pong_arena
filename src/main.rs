//! Pong Arena — Pong LAN adattivo.
//!
//! Con 2 giocatori è il Pong classico (campo rettangolare). Con 3 o più
//! giocatori il campo diventa un poligono regolare a N lati: ognuno difende un
//! lato e ha 7 vite ("schiaccia 7"). Chi le esaurisce viene eliminato e il suo
//! lato si chiude; vince l'ultimo rimasto.
//!
//! Uso:
//!   pong_arena host [--port N] [--bots K]
//!   pong_arena join <ip> [--port N]

mod app;
mod arena;
mod game;
mod geom;
mod net;
mod render;
mod terminal;

const DEFAULT_PORT: u16 = 7878;

fn usage() -> ! {
    eprintln!(
        "Pong Arena — Pong LAN adattivo (2 = classico, 3+ = arena poligonale)\n\
         \n\
         USO:\n  \
           pong_arena host [--port N] [--bots K]   avvia il server e la lobby\n  \
           pong_arena join <ip> [--port N]         unisciti a un host in LAN\n\
         \n\
         OPZIONI:\n  \
           --port N    porta TCP (default {DEFAULT_PORT})\n  \
           --bots K    riempi K posti con avversari IA (utile per provare da soli)\n  \
           -h, --help  mostra questo aiuto\n\
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

fn parse_opt_port(args: &[String], i: &mut usize) -> Option<u16> {
    // Cerca --port tra gli argomenti rimanenti.
    let mut port = DEFAULT_PORT;
    let mut bots = 0usize;
    let mut k = *i;
    let mut found_bots = None;
    while k < args.len() {
        match args[k].as_str() {
            "--port" => {
                k += 1;
                port = args.get(k)?.parse().ok()?;
            }
            "--bots" => {
                k += 1;
                bots = args.get(k)?.parse().ok()?;
                found_bots = Some(bots);
            }
            _ => return None,
        }
        k += 1;
    }
    let _ = found_bots;
    *i = k;
    // Codifichiamo bots nei bit alti? No: gestiamo separatamente. Vedi main.
    // (Questa funzione è usata solo per il caso join, dove bots è ignorato.)
    let _ = bots;
    Some(port)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
    }

    match args[0].as_str() {
        "host" => {
            // Parsing manuale di --port / --bots.
            let mut port = DEFAULT_PORT;
            let mut bots = 0usize;
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
                    _ => usage(),
                }
                i += 1;
            }
            // Limite di posti: 1 host + bot non può superare 8.
            if bots > 7 {
                bots = 7;
            }
            if let Err(e) = app::run_host(port, bots) {
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
            let port = match parse_opt_port(&args, &mut i) {
                Some(p) => p,
                None => usage(),
            };
            if let Err(e) = app::run_guest(&addr, port) {
                eprintln!("Errore guest: {e}");
                std::process::exit(1);
            }
        }
        _ => usage(),
    }
}
