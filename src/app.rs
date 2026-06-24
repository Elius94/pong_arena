//! Orchestrazione: lobby, loop host (server autoritativo) e loop guest.

use crate::game::*;
use crate::net::*;
use crate::render::{self, Frame};
use crate::terminal::{poll_key, InputState, Key, TerminalGuard};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FRAME: Duration = Duration::from_micros(16_667); // ~60 fps
const TRAIL_LEN: usize = 12;
const MAX_PLAYERS: usize = 20;

fn seed() -> u64 {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    n ^ (n >> 29) ^ 0xD1B5_4A32_D192_ED03
}

fn term_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((80, 24))
}

fn target_aspect(n: usize) -> f32 {
    if n <= 2 {
        2.0
    } else {
        1.0
    }
}

// ---------------------------------------------------------------------------
// Composizione della schermata di gioco.
// ---------------------------------------------------------------------------
fn compose(snap: &Snapshot, my_id: usize, title_right: &str, names: &[String]) -> String {
    let (cols, rows) = term_size();
    let layout = render::layout(cols, rows, target_aspect(snap.n));
    let (cw, ch, oc, or_) = match layout {
        Some(x) => x,
        None => return render::too_small(cols as usize, rows as usize),
    };

    // Stato eventuale (spettatore / fine partita).
    let mut status_owned: Option<String> = None;
    if snap.phase_code != 2 {
        if my_id < snap.players.len() && !snap.players[my_id].2 {
            status_owned = Some("sei stato eliminato — spettatore".to_string());
        }
    }

    let mut out = String::from("\x1b[2J");
    let mut frame = Frame::new(cw, ch);
    let trail = TRAIL.with(|t| t.borrow().clone());
    render::draw_arena(&mut frame, snap, my_id, &trail);
    out.push_str(&render::blit(&frame, oc, or_));
    out.push_str(&render::chrome(
        cols as usize,
        rows as usize,
        title_right,
        Some(snap),
        my_id,
        status_owned.as_deref(),
        names,
    ));
    if snap.phase_code == 2 {
        out.push_str(&render::game_over_overlay(
            cols as usize,
            rows as usize,
            snap.winner.max(0) as usize,
            my_id,
            names,
        ));
    }
    // Overlay granata: mostrato solo al giocatore congelato da una granata avversaria.
    if let Some(&(_, _, freeze_t, _, cap)) = snap.weapons.get(my_id) {
        if freeze_t > 0.0 && cap & 0x04 != 0 {
            out.push_str(&render::grenade_overlay(cols as usize, rows as usize, freeze_t));
        }
    }
    out
}

// Scia condivisa per il rendering locale (semplice, thread-local).
thread_local! {
    static TRAIL: std::cell::RefCell<VecDeque<(f32, f32)>> =
        std::cell::RefCell::new(VecDeque::new());
}

fn push_trail(p: (f32, f32)) {
    TRAIL.with(|t| {
        let mut t = t.borrow_mut();
        t.push_back(p);
        while t.len() > TRAIL_LEN {
            t.pop_front();
        }
    });
}

fn clear_trail() {
    TRAIL.with(|t| t.borrow_mut().clear());
}

// ---------------------------------------------------------------------------
// Lobby dell'host.
// ---------------------------------------------------------------------------
fn render_lobby(port: u16, n_conn: usize, bots: usize) -> String {
    let (cols, rows) = term_size();
    let cols = cols as usize;
    let total = (1 + n_conn + bots).min(MAX_PLAYERS);
    let mut out = String::from("\x1b[2J");
    let accent = (90, 224, 205);
    let dim = (150, 158, 178);
    let warm = (250, 210, 120);

    let mut line = |row: usize, col: u8, s: &str, c: (u8, u8, u8)| {
        let _ = col;
        let l = s.chars().count();
        let cc = cols.saturating_sub(l) / 2;
        out.push_str(&format!(
            "\x1b[{};{}H\x1b[38;2;{};{};{}m{}\x1b[0m",
            row + 1,
            cc + 1,
            c.0,
            c.1,
            c.2,
            s
        ));
    };

    let mid = (rows as usize) / 2;
    line(mid.saturating_sub(5), 0, "▌ PONG · ARENA ▐", accent);
    line(
        mid.saturating_sub(3),
        0,
        &format!("in ascolto sulla porta {port}"),
        dim,
    );
    line(
        mid.saturating_sub(1),
        0,
        &format!("giocatori collegati: {n_conn}    bot: {bots}"),
        dim,
    );
    let modeword = if total <= 2 {
        "duello classico (rettangolo)"
    } else {
        "arena poligonale a N lati"
    };
    line(
        mid,
        0,
        &format!("totale in partita: {total}  →  {modeword}"),
        warm,
    );
    if total >= 2 {
        line(mid + 2, 0, "[INVIO] avvia la partita     [Q] esci", accent);
    } else {
        line(
            mid + 2,
            0,
            "serve almeno 1 avversario (o un bot)     [Q] esci",
            dim,
        );
    }
    line(
        mid + 4,
        0,
        "comunica agli altri il tuo IP LAN (ip addr / ipconfig)",
        dim,
    );
    out
}

// ---------------------------------------------------------------------------
// Host.
// ---------------------------------------------------------------------------
pub fn run_host(port: u16, bots: usize, host_name: String, lives: i32) -> std::io::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    listener.set_nonblocking(true)?;

    let _guard = TerminalGuard::new()?;
    let mut clients: Vec<TcpStream> = Vec::new();
    let mut guest_names: Vec<String> = Vec::new();
    let mut last_count = usize::MAX;

    // --- Lobby ---
    let start = loop {
        // Accetta connessioni in attesa.
        loop {
            match listener.accept() {
                Ok((s, _addr)) => {
                    let _ = s.set_nodelay(true);
                    // Su Windows i socket accettati ereditano il flag non-blocking
                    // del listener: forziamo esplicitamente la modalità blocking.
                    let _ = s.set_nonblocking(false);
                    // Timeout brevissimo sui read per la lobby: non usiamo
                    // non-blocking perché su Windows può impedire i write
                    // successivi (i write devono restare blocking).
                    let _ = s.set_read_timeout(Some(Duration::from_millis(1)));
                    if 1 + clients.len() + bots < MAX_PLAYERS {
                        clients.push(s);
                        guest_names.push(String::new()); // placeholder
                    }
                    // oltre il limite: il socket viene chiuso (drop)
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        // Leggi eventuali messaggi NAME dai client (non-blocking).
        for (ci, client) in clients.iter_mut().enumerate() {
            let mut buf = [0u8; 256];
            loop {
                match client.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let msg = String::from_utf8_lossy(&buf[..n]);
                        for line in msg.lines() {
                            if let Some(name) = line.strip_prefix("NAME ") {
                                let name = name.trim().to_string();
                                if !name.is_empty() {
                                    guest_names[ci] = name;
                                }
                            }
                        }
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        break
                    }
                    Err(_) => break,
                }
            }
        }

        // Notifica i client del conteggio se cambiato.
        if clients.len() != last_count {
            last_count = clients.len();
            let msg = format!("LOBBY {}\n", 1 + clients.len() + bots);
            for c in clients.iter_mut() {
                let _ = c.write_all(msg.as_bytes());
                let _ = c.flush();
            }
        }

        render::flush(&render_lobby(port, clients.len(), bots))?;

        if let Some(k) = poll_key()? {
            match k {
                Key::Enter | Key::Space => {
                    if 1 + clients.len() + bots >= 2 {
                        break true;
                    }
                }
                Key::Quit => break false,
                Key::Other => {}
            }
        }
        std::thread::sleep(Duration::from_millis(40));
    };

    if !start {
        return Ok(());
    }

    // --- Assegnazione posti ---
    let total = (1 + clients.len() + bots).min(MAX_PLAYERS).max(2);
    let n = total;
    let guests_used = (n - 1).min(clients.len());
    let bots_used = n - 1 - guests_used;

    // Costruisci lista nomi: host + guests + bot.
    let mut all_names: Vec<String> = Vec::with_capacity(n);
    all_names.push(host_name);
    for i in 0..guests_used {
        let name = if guest_names[i].is_empty() {
            format!("Giocatore {}", i + 1)
        } else {
            guest_names[i].clone()
        };
        all_names.push(name);
    }
    for i in 0..bots_used {
        all_names.push(format!("Bot {}", i + 1));
    }

    // Strutture indicizzate per pid.
    let mut writers: Vec<Option<TcpStream>> = (0..n).map(|_| None).collect();
    let mut inputs: Vec<Option<InputChannel>> = (0..n).map(|_| None).collect();
    let mut is_bot: Vec<bool> = vec![false; n];

    // Guest: pid 1..=guests_used.
    for (i, mut s) in clients.into_iter().enumerate() {
        if i >= guests_used {
            // socket in eccesso: chiuso automaticamente al drop
            continue;
        }
        let pid = i + 1;
        // Rimuovi il timeout di lettura: da qui in poi tutto è blocking.
        let _ = s.set_read_timeout(None);
        let start_msg = format!("START {} {} {} {}\n", pid, n, lives, all_names[pid]);
        s.write_all(start_msg.as_bytes())?;
        s.flush()?;
        // Invia la lista nomi a questo guest.
        let names_msg = format!("NAMES {}\n", all_names.join("|"));
        s.write_all(names_msg.as_bytes())?;
        s.flush()?;
        let reader = BufReader::new(s.try_clone()?);
        inputs[pid] = Some(spawn_input_reader(reader));
        writers[pid] = Some(s);
    }
    // Bot: pid finali.
    for pid in (1 + guests_used)..n {
        is_bot[pid] = true;
    }
    let _ = bots_used;

    // --- Game loop ---
    let mut game = GameState::new(n, seed(), lives);
    game.set_names(&all_names);
    let mut input = InputState::new();
    let mut last = Instant::now();
    let mut last_size = term_size();
    clear_trail();

    let title = format!("{} · {} giocatori · porta {}", all_names[0], n, port);

    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.05);
        last = now;

        input.pump()?;
        if input.quit {
            break;
        }

        // Disconnessioni dei guest.
        for pid in 1..n {
            if let Some(ch) = &inputs[pid] {
                if ch.is_disconnected() && game.players[pid].connected {
                    game.mark_disconnected(pid);
                    writers[pid] = None;
                }
            }
        }

        // Restart (solo a fine partita) da host o da un qualsiasi guest.
        // Raccoglie anche le azioni armi dei guest (edge-triggered).
        let mut want_restart = input.restart;
        let mut guest_actions: Vec<(bool, bool)> = vec![(false, false); n];
        for pid in 1..n {
            if let Some(ch) = &inputs[pid] {
                let (r, _q, fire, grenade) = ch.take_edges();
                want_restart |= r;
                guest_actions[pid] = (fire, grenade);
            }
        }
        if matches!(game.phase, Phase::GameOver(_)) && want_restart {
            game.restart();
            clear_trail();
        }
        input.restart = false;

        // Movimento e armi: host (pid 0), guest (input di rete), bot (IA).
        game.apply_input(0, input.intent, dt);
        game.apply_action(0, input.fire, input.grenade);
        input.fire = false;
        input.grenade = false;
        for pid in 1..n {
            if is_bot[pid] {
                game.bot_step(pid, dt);
                game.bot_action(pid);
            } else if let Some(ch) = &inputs[pid] {
                game.apply_input(pid, ch.get().intent, dt);
                let (fire, grenade) = guest_actions[pid];
                game.apply_action(pid, fire, grenade);
            }
        }

        game.step(dt);

        // Snapshot a tutti i client.
        let snap = game.snapshot();
        let line = snap.encode();
        for pid in 1..n {
            if let Some(w) = writers[pid].as_mut() {
                if w.write_all(line.as_bytes()).is_err() || w.flush().is_err() {
                    // marca disconnesso al prossimo giro tramite il reader
                    writers[pid] = None;
                }
            }
        }

        // Scia + render locale.
        if snap.phase_code == 1 {
            if let Some(b) = snap.balls.first() {
                push_trail((b.x, b.y));
            }
        }
        let size = term_size();
        if size != last_size {
            last_size = size;
            render::flush("\x1b[2J")?;
        }
        render::flush(&compose(&snap, 0, &title, &all_names))?;

        let elapsed = now.elapsed();
        if elapsed < FRAME {
            std::thread::sleep(FRAME - elapsed);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Guest.
// ---------------------------------------------------------------------------
pub fn run_guest(addr: &str, port: u16, my_name: String) -> std::io::Result<()> {
    println!("Connessione a {addr}:{port} …");
    let stream = TcpStream::connect((addr, port))?;
    stream.set_nodelay(true)?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    // Invia il proprio nickname all'host.
    writer.write_all(format!("NAME {}\n", my_name).as_bytes())?;
    writer.flush()?;

    // Attendi l'handshake START (durante la lobby arrivano righe LOBBY).
    println!("Connesso. In attesa che l'host avvii la partita…");
    let (my_id, n, mut all_names) = loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            println!("L'host ha chiuso la connessione.");
            return Ok(());
        }
        let line = line.trim_end();
        if let Some(rest) = line.strip_prefix("START ") {
            let mut it = rest.split_whitespace();
            let id: usize = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let n: usize = it.next().and_then(|v| v.parse().ok()).unwrap_or(2);
            // Ignora eventuali campi extra (lives, name) nel messaggio START.
            // La prossima riga dovrebbe essere NAMES.
            let mut names_line = String::new();
            reader.read_line(&mut names_line)?;
            let names_str = names_line.trim().strip_prefix("NAMES ").unwrap_or("");
            let names: Vec<String> = names_str.split('|').map(|s| s.to_string()).collect();
            break (id, n, names);
        } else if let Some(rest) = line.strip_prefix("LOBBY ") {
            println!("  giocatori in lobby: {}", rest.trim());
        }
    };

    let _guard = TerminalGuard::new()?;
    let chan = spawn_snapshot_reader(reader);
    let mut writer = writer;
    let mut input = InputState::new();
    let mut last_size = term_size();

    // Completa i nomi mancanti con default.
    while all_names.len() < n {
        all_names.push(format!("Giocatore {}", all_names.len() + 1));
    }

    let title = format!("{} · giocatore {} di {}", all_names[my_id], my_id + 1, n);
    clear_trail();

    loop {
        let frame_start = Instant::now();
        input.pump()?;

        // Invia il proprio input.
        let ni = NetInput {
            intent: input.intent,
            restart: input.restart,
            quit: input.quit,
            fire: input.fire,
            grenade: input.grenade,
        };
        let _ = writer.write_all(ni.encode().as_bytes());
        let _ = writer.flush();
        input.restart = false;
        input.fire = false;
        input.grenade = false;

        if input.quit {
            break;
        }

        if chan.is_disconnected() {
            let (cols, rows) = term_size();
            let mut s = String::from("\x1b[2J");
            let msg = "Connessione persa con l'host";
            let cc = (cols as usize).saturating_sub(msg.len()) / 2;
            s.push_str(&format!(
                "\x1b[{};{}H\x1b[38;2;240;120;120m{}\x1b[0m",
                rows / 2,
                cc + 1,
                msg
            ));
            render::flush(&s)?;
            // attende un tasto per uscire
            loop {
                if poll_key()?.is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            break;
        }

        if let Some(snap) = chan.get() {
            if snap.phase_code == 1 {
                if let Some(b) = snap.balls.first() {
                    push_trail((b.x, b.y));
                }
            }
            let size = term_size();
            if size != last_size {
                last_size = size;
                render::flush("\x1b[2J")?;
            }
            render::flush(&compose(&snap, my_id, &title, &all_names))?;
        }

        let elapsed = frame_start.elapsed();
        if elapsed < FRAME {
            std::thread::sleep(FRAME - elapsed);
        }
    }

    // Saluto finale: comunica l'uscita all'host.
    let _ = writer.write_all(
        NetInput {
            intent: 0,
            restart: false,
            quit: true,
            fire: false,
            grenade: false,
        }
        .encode()
        .as_bytes(),
    );
    let _ = writer.flush();
    Ok(())
}
