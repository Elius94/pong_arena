//! Orchestrazione: lobby, loop host (server autoritativo) e loop guest.

use crate::game::*;
use crate::net::*;
use crate::render::{self, Frame};
use crate::replay;
use crate::scores;
use crate::terminal::{poll_key, InputState, Key, TerminalGuard};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FRAME: Duration = Duration::from_micros(16_667); // ~60 fps
const TRAIL_LEN: usize = 12;
const MAX_PLAYERS: usize = 40;

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

    // Sovrascrive le righe fuori dall'area di gioco (chrome) con spazi a larghezza
    // piena anziché usare \x1b[2K. Su Windows ConHost le sequenze "erase" causano
    // un riempimento del buffer che viene renderizzato come flash visibile, mentre
    // la scrittura di caratteri (spazi) viene raggruppata nel singolo flush() e
    // non produce flash.
    let rows_n = rows as usize;
    let blank_row = format!("\x1b[0m{}", " ".repeat(cols as usize));
    let mut out = String::new();
    // Righe 0 e 1 (titolo e lista giocatori) e ultima riga sono gestite da chrome(),
    // non vengono azzerate qui per evitare il doppio-write che causa flickering su Windows.
    for r in 2..or_ {
        out.push_str(&format!("\x1b[{};1H{}", r + 1, blank_row));
    }
    for r in (or_ + ch)..rows_n.saturating_sub(1) {
        out.push_str(&format!("\x1b[{};1H{}", r + 1, blank_row));
    }
    // Pickup animation: confronta items col frame precedente, lancia animazioni per quelli scomparsi.
    let curr_items: Vec<(f32, f32, u8)> = snap.items.iter()
        .map(|(pos, k)| (pos.x, pos.y, *k)).collect();
    PICKUP_ANIMS.with(|anims_cell| {
        PREV_ITEMS.with(|prev_cell| {
            let prev = prev_cell.borrow();
            let mut anims = anims_cell.borrow_mut();
            for &(px, py, pk) in prev.iter() {
                if !snap.items.iter().any(|(pos, k)| {
                    (pos.x - px).abs() < 0.5 && (pos.y - py).abs() < 0.5 && *k == pk
                }) {
                    anims.push(PickupAnim { wx: px, wy: py, kind: pk, timer: 0.7 });
                }
            }
            for a in anims.iter_mut() { a.timer -= 1.0 / 60.0; }
            anims.retain(|a| a.timer > 0.0);
        });
    });
    PREV_ITEMS.with(|p| *p.borrow_mut() = curr_items);
    let anim_list: Vec<(f32, f32, u8, f32)> = PICKUP_ANIMS.with(|a| {
        a.borrow().iter().map(|a| (a.wx, a.wy, a.kind, a.timer)).collect()
    });

    let mut frame = Frame::new(cw, ch);
    let trail = TRAIL.with(|t| t.borrow().clone());
    render::draw_arena(&mut frame, snap, my_id, &trail);
    out.push_str(&render::blit(&frame, oc, or_));
    out.push_str(&render::chrome(
        cols as usize,
        rows_n,
        title_right,
        Some(snap),
        my_id,
        status_owned.as_deref(),
        names,
    ));
    out.push_str(&render::player_name_labels(
        snap, names, cw, ch, oc, or_, my_id, cols as usize, rows_n,
    ));
    out.push_str(&render::pickup_anim_overlay(
        &anim_list, snap.n, my_id, cw, ch, oc, or_,
    ));
    if snap.phase_code == 2 {
        out.push_str(&render::game_over_overlay(
            cols as usize,
            rows_n,
            snap.winner.max(0) as usize,
            my_id,
            names,
        ));
    }
    // Overlay granata: sempre chiamato così ripulisce i bordi quando l'effetto finisce.
    let grenade_freeze_t = snap.weapons.get(my_id)
        .map(|&(_, _, ft, _, cap, _, _, _)| if ft > 0.0 && cap & 0x04 != 0 { ft } else { 0.0 })
        .unwrap_or(0.0);
    out.push_str(&render::grenade_overlay(cols as usize, rows_n, or_, oc, cw, grenade_freeze_t));
    out
}

// Scia condivisa + animazioni raccolta item (thread-local, singolo thread di rendering).
struct PickupAnim {
    wx: f32,
    wy: f32,
    kind: u8,
    timer: f32, // scende da 0.7 a 0.0
}

thread_local! {
    static TRAIL: std::cell::RefCell<VecDeque<(f32, f32)>> =
        std::cell::RefCell::new(VecDeque::new());
    static PICKUP_ANIMS: std::cell::RefCell<Vec<PickupAnim>> =
        std::cell::RefCell::new(Vec::new());
    static PREV_ITEMS: std::cell::RefCell<Vec<(f32, f32, u8)>> =
        std::cell::RefCell::new(Vec::new());
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
    let n_human_guests = guests_used;

    // --- Game loop ---
    let mut replay_frames: Vec<String> = Vec::new();
    let mut game = GameState::new(n, seed(), lives);
    game.set_names(&all_names);
    let mut input = InputState::new();
    let mut last = Instant::now();
    let mut last_size = term_size();
    let mut score_saved = false;
    clear_trail();

    let title = format!("{} · {} giocatori · porta {}", all_names[0], n, port);

    // Pausa: None=in gioco, Some(false)=menu pausa solo, Some(true)=abbandono multi
    let mut pause_mode: Option<bool> = None;
    let mut pause_sel: usize = 0;
    let mut nav_cd = 0.0f32;

    // Spettatori che si connettono a partita già avviata
    let mut spec_writers: Vec<TcpStream> = Vec::new();

    // UDP broadcast per scoperta LAN
    let udp_sock: Option<std::net::UdpSocket> =
        std::net::UdpSocket::bind("0.0.0.0:0").ok().and_then(|s| {
            s.set_broadcast(true).ok()?;
            s.set_nonblocking(true).ok()?;
            Some(s)
        });
    let mut udp_timer = 0.0f32;

    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.05);
        last = now;
        nav_cd -= dt;

        input.pump()?;
        if input.quit { break; } // Ctrl+C: uscita immediata

        // Gestione pausa / abbandono
        if input.pause {
            input.pause = false;
            if pause_mode.is_none() {
                pause_mode = Some(n_human_guests > 0);
                pause_sel = 0;
            } else {
                pause_mode = None; // ESC chiude il menu senza azione
            }
        }
        if pause_mode.is_some() {
            if nav_cd <= 0.0 && input.intent != 0 {
                pause_sel = 1 - pause_sel;
                nav_cd = 0.20;
            }
            if input.confirm {
                input.confirm = false;
                if pause_sel == 1 { break; } else { pause_mode = None; }
            }
        }
        input.confirm = false;

        // Accetta nuove connessioni a partita in corso come spettatori
        loop {
            match listener.accept() {
                Ok((mut s, _)) => {
                    let _ = s.set_nodelay(true);
                    let _ = s.set_nonblocking(false);
                    let _ = s.set_write_timeout(Some(Duration::from_millis(100)));
                    let spec_msg  = format!("SPEC {}\n", n);
                    let names_msg = format!("NAMES {}\n", all_names.join("|"));
                    if s.write_all(spec_msg.as_bytes()).is_ok()
                        && s.write_all(names_msg.as_bytes()).is_ok()
                        && s.flush().is_ok()
                    {
                        spec_writers.push(s);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                _ => break,
            }
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
            score_saved = false;
            clear_trail();
        }
        input.restart = false;

        // Movimento e armi — saltato se in pausa solo-player
        let is_solo_paused = matches!(pause_mode, Some(false));
        if !is_solo_paused {
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
        } else {
            input.fire = false;
            input.grenade = false;
        }

        // Salva la classifica al primo frame di GameOver.
        if !score_saved {
            if let Phase::GameOver(winner) = game.phase {
                let winner_name = all_names.get(winner).map(|s| s.as_str()).unwrap_or("");
                scores::update(winner_name, &all_names, &game.kills);
                score_saved = true;
            }
        }

        // Snapshot a tutti i client e agli spettatori.
        let snap = game.snapshot();
        let line = snap.encode();
        replay_frames.push(line.clone());
        for pid in 1..n {
            if let Some(w) = writers[pid].as_mut() {
                if w.write_all(line.as_bytes()).is_err() || w.flush().is_err() {
                    writers[pid] = None;
                }
            }
        }
        spec_writers.retain_mut(|sw| {
            sw.write_all(line.as_bytes()).is_ok() && sw.flush().is_ok()
        });

        // UDP broadcast per scoperta LAN
        udp_timer -= dt;
        if udp_timer <= 0.0 {
            udp_timer = 2.0;
            let conn_count = game.players.iter().filter(|p| p.connected).count();
            let msg = format!("PONG_ARENA v1 {} {}/{}\n", port, conn_count, n);
            if let Some(ref sock) = udp_sock {
                let _ = sock.send_to(msg.as_bytes(), ("255.255.255.255", DISCOVERY_PORT));
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
        let mut frame_str = compose(&snap, 0, &title, &all_names);
        if let Some(is_abandon) = pause_mode {
            let (cols, rows) = term_size();
            frame_str.push_str(&render::pause_overlay(
                cols as usize, rows as usize, is_abandon, true, pause_sel,
            ));
        }
        render::flush(&frame_str)?;

        let elapsed = now.elapsed();
        if elapsed < FRAME {
            std::thread::sleep(FRAME - elapsed);
        }
    }

    // Salva il replay se la partita ha avuto abbastanza frame (> 5 secondi circa)
    if replay_frames.len() > 300 {
        let _ = replay::save(&replay_frames, &all_names);
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

    // Attendi START (normale), SPEC (spettatore mid-game) o LOBBY (in attesa).
    println!("Connesso. In attesa che l'host avvii la partita…");
    let (my_id, n, mut all_names, is_spectator) = loop {
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
            let n: usize  = it.next().and_then(|v| v.parse().ok()).unwrap_or(2);
            let mut names_line = String::new();
            reader.read_line(&mut names_line)?;
            let names_str = names_line.trim().strip_prefix("NAMES ").unwrap_or("");
            let names: Vec<String> = names_str.split('|').map(|s| s.to_string()).collect();
            break (id, n, names, false);
        } else if let Some(rest) = line.strip_prefix("SPEC ") {
            // Partita già in corso: entra come spettatore
            let n: usize = rest.trim().parse().unwrap_or(2);
            let mut names_line = String::new();
            reader.read_line(&mut names_line)?;
            let names_str = names_line.trim().strip_prefix("NAMES ").unwrap_or("");
            let names: Vec<String> = names_str.split('|').map(|s| s.to_string()).collect();
            break (n, n, names, true); // my_id = n (sentinella fuori-bounds = no HUD personale)
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

    let title = if is_spectator {
        format!("SPETTATORE — {} giocatori", n)
    } else {
        format!("{} · giocatore {} di {}", all_names[my_id], my_id + 1, n)
    };
    clear_trail();

    // Stato abbandono partita (Q/ESC)
    let mut show_abandon = false;
    let mut abandon_sel: usize = 0;
    let mut nav_cd = 0.0f32;
    let mut last_frame = Instant::now();

    loop {
        let frame_start = Instant::now();
        let dt = (frame_start - last_frame).as_secs_f32().min(0.05);
        last_frame = frame_start;
        nav_cd -= dt;

        input.pump()?;
        if input.quit { break; } // Ctrl+C: uscita immediata

        // ESC/Q → conferma abbandono (spettatori escono direttamente)
        if input.pause {
            input.pause = false;
            if is_spectator {
                break;
            }
            show_abandon = !show_abandon;
            if show_abandon { abandon_sel = 0; nav_cd = 0.0; }
        }

        // Navigazione menu abbandono
        if show_abandon {
            if nav_cd <= 0.0 && input.intent != 0 {
                abandon_sel = 1 - abandon_sel;
                nav_cd = 0.20;
            }
            if input.confirm {
                input.confirm = false;
                if abandon_sel == 1 { break; } else { show_abandon = false; }
            }
        }
        input.confirm = false;

        // Invia il proprio input (non per spettatori, non durante menu abbandono)
        if !is_spectator {
            let ni = NetInput {
                intent: if show_abandon { 0 } else { input.intent },
                restart: input.restart,
                quit: false,
                fire: if show_abandon { false } else { input.fire },
                grenade: if show_abandon { false } else { input.grenade },
            };
            let _ = writer.write_all(ni.encode().as_bytes());
            let _ = writer.flush();
        }
        input.restart = false;
        input.fire = false;
        input.grenade = false;

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
            loop {
                if poll_key()?.is_some() { break; }
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
            // Spettatori usano my_id = n (nessun HUD personale; vista neutra)
            let render_id = if is_spectator { snap.n } else { my_id };
            let mut frame_str = compose(&snap, render_id, &title, &all_names);
            let (cols, rows) = term_size();
            if is_spectator {
                frame_str.push_str(&render::spectator_overlay(cols as usize, rows as usize));
            } else if show_abandon {
                frame_str.push_str(&render::pause_overlay(
                    cols as usize, rows as usize, true, false, abandon_sel,
                ));
            }
            render::flush(&frame_str)?;
        }

        let elapsed = frame_start.elapsed();
        if elapsed < FRAME {
            std::thread::sleep(FRAME - elapsed);
        }
    }

    // Saluto finale: comunica l'uscita all'host.
    if !is_spectator {
        let _ = writer.write_all(
            NetInput { intent: 0, restart: false, quit: true, fire: false, grenade: false }
                .encode()
                .as_bytes(),
        );
        let _ = writer.flush();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Replay.
// ---------------------------------------------------------------------------
pub fn run_replay(path: &std::path::Path) -> std::io::Result<()> {
    let rep = replay::load(path).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, format!("replay: {e}"))
    })?;
    if rep.frames.is_empty() {
        return Ok(());
    }

    let _guard = TerminalGuard::new()?;
    let mut input = InputState::new();
    let mut frame_idx: usize = 0;
    let mut speed: usize = 1; // 1x 2x 4x 8x 16x
    let mut paused = false;
    let mut last = Instant::now();
    let mut last_size = term_size();
    let mut speed_cd = 0.0f32;
    clear_trail();

    let total = rep.frames.len();

    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.05);
        last = now;
        speed_cd -= dt;

        input.pump()?;

        // Ctrl+C o Q/ESC → esci
        if input.quit || input.pause { break; }

        // Space (fire) o Enter (confirm) → pausa / riprendi
        if input.fire || input.confirm {
            paused = !paused;
        }
        input.fire = false;
        input.confirm = false;

        // R → ricomincia dall'inizio
        if input.restart {
            input.restart = false;
            frame_idx = 0;
            paused = false;
            clear_trail();
        }

        // ←/→ → cambia velocità (con cooldown per evitare cambi multipli a frame)
        if speed_cd <= 0.0 && input.intent != 0 {
            if input.intent > 0 { speed = (speed * 2).min(16); }
            else                { speed = (speed / 2).max(1);  }
            speed_cd = 0.25;
        }

        // Avanza i frame
        if !paused && frame_idx < total.saturating_sub(1) {
            frame_idx = (frame_idx + speed).min(total - 1);
        }
        if frame_idx >= total - 1 {
            paused = true;
        }

        // Rendering
        let frame_line = &rep.frames[frame_idx];
        if let Some(snap) = Snapshot::decode(frame_line) {
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
            let pct = if total > 1 { frame_idx * 100 / (total - 1) } else { 100 };
            let icon = if paused { "⏸" } else { "▶" };
            let title = format!(
                "{} ×{}  {}/{}  {}%",
                icon, speed, frame_idx + 1, total, pct
            );
            let frame_str = compose(&snap, 0, &title, &rep.names);
            render::flush(&frame_str)?;
        }

        let elapsed = now.elapsed();
        if elapsed < FRAME {
            std::thread::sleep(FRAME - elapsed);
        }
    }

    Ok(())
}
