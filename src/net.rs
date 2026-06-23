//! Networking: thread reader che non bloccano il game loop.
//!
//! Host: per ogni client un thread legge gli input e ne tiene l'ultimo.
//! Guest: un thread legge gli snapshot e ne tiene l'ultimo.
//! Ogni canale espone anche un flag di disconnessione.

use crate::game::{NetInput, Snapshot};
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Canale verso un client (lato host): ultimo input ricevuto + stato.
pub struct InputChannel {
    last: Arc<Mutex<NetInput>>,
    disconnected: Arc<AtomicBool>,
}

impl InputChannel {
    pub fn get(&self) -> NetInput {
        *self.last.lock().unwrap()
    }
    pub fn is_disconnected(&self) -> bool {
        self.disconnected.load(Ordering::Relaxed)
    }
}

/// Avvia il thread che legge gli input dal client. Il `BufReader` deve essere
/// già posizionato oltre l'handshake (START) per non perdere byte bufferizzati.
pub fn spawn_input_reader(reader: BufReader<TcpStream>) -> InputChannel {
    let last = Arc::new(Mutex::new(NetInput {
        intent: 0,
        restart: false,
        quit: false,
        fire: false,
        grenade: false,
    }));
    let disconnected = Arc::new(AtomicBool::new(false));
    let last_c = last.clone();
    let dis_c = disconnected.clone();

    thread::spawn(move || {
        let mut reader = reader;
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if let Some(inp) = NetInput::decode(line.trim_end()) {
                        let mut g = last_c.lock().unwrap();
                        // I flag edge si accumulano finché il game loop non
                        // li consuma tramite take_edges().
                        let prev = *g;
                        *g = NetInput {
                            intent: inp.intent,
                            restart: prev.restart || inp.restart,
                            quit: prev.quit || inp.quit,
                            fire: prev.fire || inp.fire,
                            grenade: prev.grenade || inp.grenade,
                        };
                        if inp.quit {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
        dis_c.store(true, Ordering::Relaxed);
    });

    InputChannel { last, disconnected }
}

impl InputChannel {
    /// Consuma i flag edge (restart, quit, fire, grenade) dopo averli letti.
    pub fn take_edges(&self) -> (bool, bool, bool, bool) {
        let mut g = self.last.lock().unwrap();
        let r = g.restart;
        let q = g.quit;
        let f = g.fire;
        let gr = g.grenade;
        g.restart = false;
        g.fire = false;
        g.grenade = false;
        // 'quit' resta vero: la disconnessione è definitiva.
        (r, q, f, gr)
    }
}

/// Canale verso il server (lato guest): ultimo snapshot ricevuto + stato.
pub struct SnapshotChannel {
    last: Arc<Mutex<Option<Snapshot>>>,
    disconnected: Arc<AtomicBool>,
}

impl SnapshotChannel {
    pub fn get(&self) -> Option<Snapshot> {
        self.last.lock().unwrap().clone()
    }
    pub fn is_disconnected(&self) -> bool {
        self.disconnected.load(Ordering::Relaxed)
    }
}

pub fn spawn_snapshot_reader(reader: BufReader<TcpStream>) -> SnapshotChannel {
    let last = Arc::new(Mutex::new(None));
    let disconnected = Arc::new(AtomicBool::new(false));
    let last_c = last.clone();
    let dis_c = disconnected.clone();

    thread::spawn(move || {
        let mut reader = reader;
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if let Some(s) = Snapshot::decode(line.trim_end()) {
                        *last_c.lock().unwrap() = Some(s);
                    }
                }
                Err(_) => break,
            }
        }
        dis_c.store(true, Ordering::Relaxed);
    });

    SnapshotChannel { last, disconnected }
}
