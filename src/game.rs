//! Stato di gioco autoritativo, fisica generica e protocollo di rete.
//!
//! La fisica è completamente indipendente dal numero di lati: la palla rimbalza
//! su una lista di muri (segmenti con normale interna). Un muro solido riflette
//! in modo speculare; un muro di un giocatore riflette se la palla colpisce la
//! racchetta, altrimenti quel giocatore subisce un punto.

use crate::arena::*;
use crate::geom::*;

// ---- Palla ----------------------------------------------------------------
pub const BALL_R: f32 = 2.0;
pub const BASE_SPEED: f32 = 82.0;
pub const MAX_SPEED: f32 = 158.0;
pub const SPEEDUP: f32 = 1.045; // accelerazione ad ogni colpo di racchetta
pub const MAX_BOUNCE: f32 = 1.0472; // ±60° rispetto alla normale del lato
const EPS: f32 = 0.01;

// ---- Armi -----------------------------------------------------------------
pub const AMMO_MAX: i32 = 10;
pub const AMMO_RELOAD_RATE: f32 = 0.2;   // munizioni al secondo (1 ogni 5 s)
pub const BULLET_SPEED: f32 = 240.0;
pub const BULLET_R: f32 = 1.2;
pub const SLOW_PER_HIT: f32 = 0.4;       // incremento slow per colpo (0.0–1.0, max a 3 colpi)
pub const SLOW_DECAY_RATE: f32 = 0.12;   // decadimento slow al secondo (recupero ~8 s)
pub const SLOW_MIN_SPEED: f32 = 0.25;    // velocità minima a slow_level=1.0
pub const BULLET_DEFLECTION: f32 = 0.7;  // deviazione tangenziale in base al movimento (~35°)
pub const FREEZE_DURATION: f32 = 2.0;    // secondi di blocco da granata
pub const GRENADES_MAX: i32 = 2;
pub const BOT_JITTER: f32 = 0.10;        // imprecisione casuale dei bot

// ---- Regole ---------------------------------------------------------------
pub const COUNTDOWN: f32 = 1.6;
#[allow(dead_code)]
pub const LIVES_START: i32 = 7; // "schiaccia 7": parti con 7 vite

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Countdown(f32),
    Playing,
    GameOver(usize), // id del vincitore
}

#[derive(Clone, Debug)]
pub struct Player {
    pub c: f32,       // centro racchetta in parametro-di-lato [0,1]
    pub lives: i32,   // vite residue
    pub alive: bool,  // ancora in gioco
    pub connected: bool,
    pub name: String, // nickname del giocatore
}

/// Stato delle armi di un singolo giocatore.
#[derive(Clone, Debug)]
pub struct WeaponState {
    pub ammo: i32,          // munizioni fucile (0–AMMO_MAX)
    pub reload_acc: f32,    // accumulatore frazionario per la ricarica
    pub slow_level: f32,    // livello di rallentamento subito (0.0 = normale, 1.0 = massimo)
    pub last_intent: i32,   // ultimo intento di movimento, usato per deviare i proiettili
    pub freeze_timer: f32,  // tempo rimanente di blocco subito (granata)
    pub grenades: i32,      // granate disponibili (0–GRENADES_MAX)
}

impl WeaponState {
    fn new() -> Self {
        WeaponState {
            ammo: AMMO_MAX,
            reload_acc: 0.0,
            slow_level: 0.0,
            last_intent: 0,
            freeze_timer: 0.0,
            grenades: 0,
        }
    }
}

/// Proiettile in volo nell'arena.
pub struct Bullet {
    pub pos: V2,
    pub vel: V2,
    pub shooter: usize,
}

// ---------------------------------------------------------------------------
// PRNG xorshift* (nessuna dipendenza esterna).
// ---------------------------------------------------------------------------
#[inline]
fn xorshift(s: &mut u64) -> u64 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *s = x;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}
#[inline]
fn rand_unit(s: &mut u64) -> f32 {
    (xorshift(s) >> 40) as f32 / (1u64 << 24) as f32
}

/// Stato completo della partita. Vive solo lato host (server autoritativo).
pub struct GameState {
    pub arena: Arena,
    pub ball_p: V2,
    pub ball_v: V2,
    pub phase: Phase,
    pub players: Vec<Player>,
    /// Segno che converte l'intento del giocatore (su/destra = +1) in
    /// incremento del parametro di lato, coerente con la vista del giocatore.
    pub param_sign: Vec<f32>,
    pub weapons: Vec<WeaponState>,
    pub bullets: Vec<Bullet>,
    last_hitter: Option<usize>,
    rng: u64,
    /// Vite iniziali salvate per il restart.
    pub start_lives: i32,
}

impl GameState {
    pub fn new(n_players: usize, seed: u64, lives: i32) -> GameState {
        let arena = Arena::new(n_players);
        let n = arena.players;
        let players = (0..n)
            .map(|_i| Player {
                c: 0.5,
                lives,
                alive: true,
                connected: true,
                name: String::new(),
            })
            .collect();
        let param_sign = Self::compute_param_sign(n);
        let weapons = (0..n).map(|_| WeaponState::new()).collect();
        let mut g = GameState {
            arena,
            ball_p: v2(0.0, 0.0),
            ball_v: v2(0.0, 0.0),
            phase: Phase::Countdown(COUNTDOWN),
            players,
            param_sign,
            weapons,
            bullets: Vec::new(),
            last_hitter: None,
            rng: seed | 1,
            start_lives: lives,
        };
        g.serve();
        g
    }

    /// Assegna i nickname ai giocatori.
    pub fn set_names(&mut self, names: &[String]) {
        for (i, p) in self.players.iter_mut().enumerate() {
            if let Some(name) = names.get(i) {
                p.name = name.clone();
            }
        }
    }

    /// Regola dei segni: nel rettangolo i due giocatori vedono il campo
    /// orizzontale (sinistra/destra), quindi il segno è opposto; nel poligono
    /// ogni giocatore vede il proprio lato in basso con il parametro che cresce
    /// verso destra, quindi il segno è sempre positivo.
    fn compute_param_sign(n: usize) -> Vec<f32> {
        if n <= 2 {
            vec![1.0, -1.0]
        } else {
            vec![1.0; n]
        }
    }

    #[allow(dead_code)]
    pub fn alive_count(&self) -> usize {
        self.players.iter().filter(|p| p.alive).count()
    }

    fn sole_survivor(&self) -> Option<usize> {
        let alive: Vec<usize> = self
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.alive)
            .map(|(i, _)| i)
            .collect();
        if alive.len() == 1 {
            Some(alive[0])
        } else {
            None
        }
    }

    /// Reimposta palla al centro e lancia verso una direzione casuale.
    fn serve(&mut self) {
        self.ball_p = v2(0.0, 0.0);
        let a = rand_unit(&mut self.rng) * std::f32::consts::TAU;
        self.ball_v = v2(a.cos(), a.sin()) * BASE_SPEED;
        self.phase = Phase::Countdown(COUNTDOWN);
        self.bullets.clear();
        self.last_hitter = None;
    }

    /// Ricomincia da capo (riapre i lati eliminati, ripristina le vite).
    pub fn restart(&mut self) {
        let n = self.players.len();
        self.arena = Arena::new(n);
        for p in self.players.iter_mut() {
            if p.connected {
                p.lives = self.start_lives;
                p.alive = true;
                p.c = 0.5;
            }
        }
        // chi era disconnesso resta fuori
        for (pid, p) in self.players.iter().enumerate() {
            if !p.connected {
                let wi = self.arena.player_wall[pid];
                self.arena.walls[wi].owner = None;
            }
        }
        for p in self.players.iter_mut() {
            if !p.connected {
                p.alive = false;
            }
        }
        for w in self.weapons.iter_mut() {
            *w = WeaponState::new();
        }
        self.bullets.clear();
        self.last_hitter = None;
        self.serve();
        if let Some(w) = self.sole_survivor() {
            self.phase = Phase::GameOver(w);
        }
    }

    /// Applica l'intento di movimento del giocatore `pid` (+1 = su/destra).
    /// La velocità è ridotta proporzionalmente allo slow_level, azzerata se congelato.
    pub fn apply_input(&mut self, pid: usize, intent: i32, dt: f32) {
        if !self.players[pid].alive {
            return;
        }
        self.weapons[pid].last_intent = intent;
        let w = &self.weapons[pid];
        let speed_mult = if w.freeze_timer > 0.0 {
            0.0
        } else {
            1.0 - (1.0 - SLOW_MIN_SPEED) * w.slow_level
        };
        let delta = self.param_sign[pid] * intent as f32 * PADDLE_PARAM_SPEED * speed_mult * dt;
        let lo = PADDLE_FRAC;
        let hi = 1.0 - PADDLE_FRAC;
        self.players[pid].c = (self.players[pid].c + delta).clamp(lo, hi);
    }

    /// Spara un colpo o usa una granata. Le azioni sono edge-triggered (un frame).
    pub fn apply_action(&mut self, pid: usize, fire: bool, grenade: bool) {
        if !matches!(self.phase, Phase::Playing) {
            return;
        }
        if !self.players[pid].alive {
            return;
        }
        if fire && self.weapons[pid].ammo > 0 {
            self.weapons[pid].ammo -= 1;
            let wi = self.arena.player_wall[pid];
            let w = self.arena.walls[wi];
            let c = self.players[pid].c;
            let origin = w.point(c) + w.n * (BULLET_R + EPS);
            // Il proiettile si devia in base al movimento: muoversi mentre si
            // spara cambia l'angolo fino a ~35°, utile per colpire avversari
            // non esattamente di fronte nel multiplayer a N giocatori.
            let intent = self.weapons[pid].last_intent as f32;
            let tangent = (w.b - w.a).norm();
            let raw_dir = w.n + tangent * (intent * self.param_sign[pid] * BULLET_DEFLECTION);
            self.bullets.push(Bullet {
                pos: origin,
                vel: raw_dir.norm() * BULLET_SPEED,
                shooter: pid,
            });
        }
        if grenade && self.weapons[pid].grenades > 0 {
            self.weapons[pid].grenades -= 1;
            for other in 0..self.players.len() {
                if other != pid && self.players[other].alive {
                    self.weapons[other].freeze_timer = FREEZE_DURATION;
                }
            }
        }
    }

    /// Muove un bot verso la proiezione della palla sul proprio lato.
    pub fn bot_step(&mut self, pid: usize, dt: f32) {
        if !self.players[pid].alive {
            return;
        }
        let wi = self.arena.player_wall[pid];
        let w = self.arena.walls[wi];
        // insegui solo se la palla si avvicina al lato
        let approaching = self.ball_v.dot(w.n) < 0.0;
        let target = if approaching && matches!(self.phase, Phase::Playing) {
            let base = project_t(self.ball_p, w.a, w.b).clamp(0.0, 1.0);
            // Piccola imprecisione casuale per evitare loop infiniti tra bot.
            let jitter = (rand_unit(&mut self.rng) - 0.5) * 2.0 * BOT_JITTER;
            (base + jitter).clamp(PADDLE_FRAC, 1.0 - PADDLE_FRAC)
        } else {
            0.5
        };
        let lo = PADDLE_FRAC;
        let hi = 1.0 - PADDLE_FRAC;
        let c = self.players[pid].c;
        let max_step = PADDLE_PARAM_SPEED * 0.85 * dt; // un filo più lenti di un umano
        let diff = target - c;
        let step = diff.clamp(-max_step, max_step);
        self.players[pid].c = (c + step).clamp(lo, hi);
    }

    pub fn mark_disconnected(&mut self, pid: usize) {
        if pid >= self.players.len() {
            return;
        }
        self.players[pid].connected = false;
        if self.players[pid].alive {
            self.eliminate(pid);
        }
        if let Some(w) = self.sole_survivor() {
            self.phase = Phase::GameOver(w);
            self.ball_v = v2(0.0, 0.0);
        }
    }

    fn eliminate(&mut self, pid: usize) {
        self.players[pid].alive = false;
        let wi = self.arena.player_wall[pid];
        self.arena.walls[wi].owner = None; // il lato diventa muro solido
    }

    fn concede(&mut self, pid: usize) {
        // Chi ha segnato riceve una granata.
        if let Some(scorer) = self.last_hitter {
            if scorer != pid && self.players[scorer].alive {
                self.weapons[scorer].grenades =
                    (self.weapons[scorer].grenades + 1).min(GRENADES_MAX);
            }
        }
        if self.players[pid].lives > 0 {
            self.players[pid].lives -= 1;
        }
        if self.players[pid].lives <= 0 && self.players[pid].alive {
            self.eliminate(pid);
        }
        if let Some(w) = self.sole_survivor() {
            self.phase = Phase::GameOver(w);
            self.ball_v = v2(0.0, 0.0);
        } else {
            self.serve();
        }
    }

    /// Avanza la simulazione di `dt` secondi.
    pub fn step(&mut self, dt: f32) {
        match self.phase {
            Phase::Countdown(t) => {
                let nt = t - dt;
                if nt <= 0.0 {
                    self.phase = Phase::Playing;
                } else {
                    self.phase = Phase::Countdown(nt);
                }
            }
            Phase::Playing => {
                self.advance_ball(dt);
                self.tick_weapons(dt);
                self.advance_bullets(dt);
            }
            Phase::GameOver(_) => {}
        }
    }

    fn advance_ball(&mut self, dt: f32) {
        let dist = self.ball_v.len() * dt;
        let steps = ((dist / 1.0).ceil() as i32).max(1);
        let sub = dt / steps as f32;
        for _ in 0..steps {
            self.ball_p = self.ball_p + self.ball_v * sub;
            // Risolvi le compenetrazioni (più passate per gli spigoli).
            for _iter in 0..4 {
                let mut worst: Option<(usize, f32, f32)> = None;
                for (i, w) in self.arena.walls.iter().enumerate() {
                    let s = (self.ball_p - w.a).dot(w.n);
                    if s < BALL_R {
                        let t = project_t(self.ball_p, w.a, w.b).clamp(0.0, 1.0);
                        let replace = match worst {
                            Some((_, bs, _)) => s < bs,
                            None => true,
                        };
                        if replace {
                            worst = Some((i, s, t));
                        }
                    }
                }
                let (wi, s, t) = match worst {
                    Some(x) => x,
                    None => break,
                };
                let w = self.arena.walls[wi];
                match w.owner {
                    None => {
                        // Muro solido: riflessione speculare.
                        let vn = self.ball_v.dot(w.n);
                        self.ball_v = self.ball_v - w.n * (2.0 * vn);
                        self.ball_p = self.ball_p + w.n * (BALL_R - s + EPS);
                    }
                    Some(pid) => {
                        let pc = self.players[pid].c;
                        if (t - pc).abs() <= PADDLE_FRAC {
                            // Colpo di racchetta: l'angolo dipende dal punto
                            // d'impatto (stile arcade), sempre verso l'interno.
                            let off = ((t - pc) / PADDLE_FRAC).clamp(-1.0, 1.0);
                            let dir = w.n.rot(off * MAX_BOUNCE);
                            let speed =
                                (self.ball_v.len() * SPEEDUP).min(MAX_SPEED).max(BASE_SPEED);
                            self.ball_v = dir * speed;
                            self.ball_p = self.ball_p + w.n * (BALL_R - s + EPS);
                            self.last_hitter = Some(pid);
                        } else {
                            self.concede(pid);
                            return;
                        }
                    }
                }
            }
        }
    }

    fn tick_weapons(&mut self, dt: f32) {
        for w in self.weapons.iter_mut() {
            if w.slow_level > 0.0 {
                w.slow_level = (w.slow_level - SLOW_DECAY_RATE * dt).max(0.0);
            }
            if w.freeze_timer > 0.0 {
                w.freeze_timer = (w.freeze_timer - dt).max(0.0);
            }
            if w.ammo < AMMO_MAX {
                w.reload_acc += dt * AMMO_RELOAD_RATE;
                let gained = w.reload_acc as i32;
                if gained > 0 {
                    w.ammo = (w.ammo + gained).min(AMMO_MAX);
                    w.reload_acc -= gained as f32;
                }
            }
        }
    }

    fn advance_bullets(&mut self, dt: f32) {
        let mut slow_targets: Vec<usize> = Vec::new();
        let mut to_remove: Vec<usize> = Vec::new();

        for (bi, bullet) in self.bullets.iter_mut().enumerate() {
            bullet.pos = bullet.pos + bullet.vel * dt;

            // Fuori dall'arena: rimuovi.
            if bullet.pos.len() > R * 3.5 {
                to_remove.push(bi);
                continue;
            }

            // Controlla collisioni con i muri.
            for w in &self.arena.walls {
                let s = (bullet.pos - w.a).dot(w.n);
                if s < BULLET_R {
                    if let Some(owner) = w.owner {
                        if owner != bullet.shooter {
                            slow_targets.push(owner);
                        }
                    }
                    to_remove.push(bi);
                    break;
                }
            }
        }

        for pid in slow_targets {
            self.weapons[pid].slow_level = (self.weapons[pid].slow_level + SLOW_PER_HIT).min(1.0);
        }
        to_remove.sort_unstable();
        to_remove.dedup();
        for bi in to_remove.into_iter().rev() {
            self.bullets.remove(bi);
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let (phase_code, countdown, winner) = match self.phase {
            Phase::Countdown(t) => (0u8, t, -1i32),
            Phase::Playing => (1, 0.0, -1),
            Phase::GameOver(w) => (2, 0.0, w as i32),
        };
        Snapshot {
            phase_code,
            countdown,
            winner,
            n: self.players.len(),
            ball: self.ball_p,
            players: self
                .players
                .iter()
                .map(|p| (p.c, p.lives, p.alive))
                .collect(),
            weapons: self
                .weapons
                .iter()
                .map(|w| (w.ammo, w.slow_level, w.freeze_timer, w.grenades))
                .collect(),
            bullets: self
                .bullets
                .iter()
                .map(|b| (b.pos, b.shooter))
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshot host → client.
// ---------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub phase_code: u8,
    pub countdown: f32,
    pub winner: i32,
    pub n: usize,
    pub ball: V2,
    pub players: Vec<(f32, i32, bool)>,            // (c, vite, vivo)
    pub weapons: Vec<(i32, f32, f32, i32)>,         // (ammo, slow, freeze, granate)
    pub bullets: Vec<(V2, usize)>,                  // (pos, shooter)
}

impl Snapshot {
    pub fn encode(&self) -> String {
        let mut s = format!(
            "S {} {:.3} {} {} {:.3} {:.3}",
            self.phase_code, self.countdown, self.winner, self.n, self.ball.x, self.ball.y
        );
        for (c, lives, alive) in &self.players {
            s.push_str(&format!(" {:.4} {} {}", c, lives, if *alive { 1 } else { 0 }));
        }
        for (ammo, slow, freeze, grenades) in &self.weapons {
            s.push_str(&format!(" {} {:.2} {:.2} {}", ammo, slow, freeze, grenades));
        }
        s.push_str(&format!(" {}", self.bullets.len()));
        for (pos, shooter) in &self.bullets {
            s.push_str(&format!(" {:.2} {:.2} {}", pos.x, pos.y, shooter));
        }
        s.push('\n');
        s
    }

    pub fn decode(line: &str) -> Option<Snapshot> {
        let mut it = line.split_whitespace();
        if it.next()? != "S" {
            return None;
        }
        let phase_code: u8 = it.next()?.parse().ok()?;
        let countdown: f32 = it.next()?.parse().ok()?;
        let winner: i32 = it.next()?.parse().ok()?;
        let n: usize = it.next()?.parse().ok()?;
        let bx: f32 = it.next()?.parse().ok()?;
        let by: f32 = it.next()?.parse().ok()?;
        let mut players = Vec::with_capacity(n);
        for _ in 0..n {
            let c: f32 = it.next()?.parse().ok()?;
            let lives: i32 = it.next()?.parse().ok()?;
            let alive: i32 = it.next()?.parse().ok()?;
            players.push((c, lives, alive != 0));
        }
        let mut weapons = Vec::with_capacity(n);
        for _ in 0..n {
            let ammo: i32 = it.next()?.parse().ok()?;
            let slow: f32 = it.next()?.parse().ok()?;
            let freeze: f32 = it.next()?.parse().ok()?;
            let grenades: i32 = it.next()?.parse().ok()?;
            weapons.push((ammo, slow, freeze, grenades));
        }
        let nb: usize = it.next()?.parse().ok()?;
        let mut bullets = Vec::with_capacity(nb);
        for _ in 0..nb {
            let px: f32 = it.next()?.parse().ok()?;
            let py: f32 = it.next()?.parse().ok()?;
            let sid: usize = it.next()?.parse().ok()?;
            bullets.push((v2(px, py), sid));
        }
        Some(Snapshot {
            phase_code,
            countdown,
            winner,
            n,
            ball: v2(bx, by),
            players,
            weapons,
            bullets,
        })
    }
}

// ---------------------------------------------------------------------------
// Input client → host.
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetInput {
    pub intent: i32, // +1 su/destra, -1 giù/sinistra, 0 fermo
    pub restart: bool,
    pub quit: bool,
    pub fire: bool,    // edge: spara col fucile
    pub grenade: bool, // edge: usa granata
}

impl NetInput {
    pub fn encode(&self) -> String {
        format!(
            "I {} {} {} {} {}\n",
            self.intent,
            self.restart as i32,
            self.quit as i32,
            self.fire as i32,
            self.grenade as i32,
        )
    }
    pub fn decode(line: &str) -> Option<NetInput> {
        let mut it = line.split_whitespace();
        if it.next()? != "I" {
            return None;
        }
        let intent: i32 = it.next()?.parse().ok()?;
        let restart: i32 = it.next()?.parse().ok()?;
        let quit: i32 = it.next()?.parse().ok()?;
        let fire: i32 = it.next()?.parse().ok()?;
        let grenade: i32 = it.next()?.parse().ok()?;
        Some(NetInput {
            intent: intent.clamp(-1, 1),
            restart: restart != 0,
            quit: quit != 0,
            fire: fire != 0,
            grenade: grenade != 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_roundtrip() {
        let g = GameState::new(4, 12345, LIVES_START);
        let s = g.snapshot();
        let back = Snapshot::decode(&s.encode()).expect("decode");
        assert_eq!(s.n, back.n);
        assert_eq!(s.players.len(), back.players.len());
        assert_eq!(s.weapons.len(), back.weapons.len());
        assert_eq!(s.bullets.len(), back.bullets.len());
        assert!((s.ball.x - back.ball.x).abs() < 1e-2);
    }

    #[test]
    fn input_roundtrip() {
        let i = NetInput {
            intent: -1,
            restart: true,
            quit: false,
            fire: true,
            grenade: false,
        };
        assert_eq!(NetInput::decode(&i.encode()), Some(i));
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(Snapshot::decode("ciao 1 2 3").is_none());
        assert!(NetInput::decode("nope").is_none());
    }

    #[test]
    fn countdown_then_play() {
        let mut g = GameState::new(3, 7, LIVES_START);
        assert!(matches!(g.phase, Phase::Countdown(_)));
        g.step(COUNTDOWN + 0.1);
        assert!(matches!(g.phase, Phase::Playing));
        assert!(g.ball_v.len() > 1.0);
    }

    #[test]
    fn missing_paddle_costs_a_life() {
        let mut g = GameState::new(3, 99, LIVES_START);
        g.phase = Phase::Playing;
        // sparo la palla dritta contro il lato del giocatore 0, lontano dalla
        // racchetta (che tengo al centro), così deve subire un punto.
        let wi = g.arena.player_wall[0];
        let w = g.arena.walls[wi];
        // punto vicino al vertice `a` del lato (parametro ~0), fuori racchetta
        let target = w.point(0.02);
        g.ball_p = target - w.n * 1.0; // appena dentro
        g.ball_v = w.n * (-BASE_SPEED); // verso l'esterno attraverso il lato
        let before = g.players[0].lives;
        for _ in 0..30 {
            g.step(1.0 / 60.0);
        }
        assert!(g.players[0].lives < before, "il giocatore 0 doveva subire un punto");
    }

    #[test]
    fn elimination_closes_wall_and_can_win() {
        let mut g = GameState::new(3, 5, LIVES_START);
        // svuoto le vite del giocatore 2
        g.players[2].lives = 1;
        // forzo l'eliminazione concedendo
        g.concede(2);
        assert!(!g.players[2].alive);
        let wi = g.arena.player_wall[2];
        assert!(g.arena.walls[wi].owner.is_none(), "il lato deve diventare solido");
        // restano 2 vivi → nessun vincitore ancora
        assert!(matches!(g.phase, Phase::Countdown(_) | Phase::Playing));
    }

    #[test]
    fn last_player_standing_wins() {
        let mut g = GameState::new(3, 5, LIVES_START);
        g.players[1].lives = 1;
        g.concede(1);
        g.players[2].lives = 1;
        g.concede(2);
        assert!(matches!(g.phase, Phase::GameOver(0)));
    }

    #[test]
    fn full_game_converges_to_a_winner() {
        let mut g = GameState::new(3, 0xDEAD_BEEF, LIVES_START);
        let mut frames = 0;
        loop {
            g.step(1.0 / 60.0);
            assert!(g.ball_p.len() < R * 2.0, "palla fuggita: {:?}", g.ball_p);
            if let Phase::GameOver(_) = g.phase {
                break;
            }
            frames += 1;
            assert!(frames < 60 * 600, "partita non conclusa in 10 minuti simulati");
        }
        assert_eq!(g.alive_count(), 1);
    }

    #[test]
    fn paddle_clamped_within_edge() {
        let mut g = GameState::new(5, 1, LIVES_START);
        for _ in 0..1000 {
            g.apply_input(0, 1, 1.0 / 60.0);
        }
        assert!(g.players[0].c <= 1.0 - PADDLE_FRAC + 1e-4);
        for _ in 0..1000 {
            g.apply_input(0, -1, 1.0 / 60.0);
        }
        assert!(g.players[0].c >= PADDLE_FRAC - 1e-4);
    }

    #[test]
    fn fire_consumes_ammo_and_creates_bullet() {
        let mut g = GameState::new(2, 42, LIVES_START);
        g.phase = Phase::Playing;
        assert_eq!(g.weapons[0].ammo, AMMO_MAX);
        g.apply_action(0, true, false);
        assert_eq!(g.weapons[0].ammo, AMMO_MAX - 1);
        assert_eq!(g.bullets.len(), 1);
    }

    #[test]
    fn grenade_freezes_opponents() {
        let mut g = GameState::new(3, 42, LIVES_START);
        g.phase = Phase::Playing;
        g.weapons[0].grenades = 1;
        g.apply_action(0, false, true);
        assert_eq!(g.weapons[0].grenades, 0);
        assert!(g.weapons[1].freeze_timer > 0.0);
        assert!(g.weapons[2].freeze_timer > 0.0);
        assert_eq!(g.weapons[0].freeze_timer, 0.0);
    }

    #[test]
    fn ammo_reloads_over_time() {
        let mut g = GameState::new(2, 1, LIVES_START);
        g.phase = Phase::Playing;
        g.weapons[0].ammo = 0;
        g.weapons[0].reload_acc = 0.0;
        // 1 munizione ogni 5 secondi (AMMO_RELOAD_RATE = 0.2); usiamo 6 s per
        // evitare problemi di arrotondamento f32.
        for _ in 0..360 {
            g.tick_weapons(1.0 / 60.0);
        }
        assert!(g.weapons[0].ammo >= 1, "doveva ricaricare almeno 1 munizione in 6 secondi");
    }

    #[test]
    fn slow_effect_reduces_paddle_speed() {
        let mut g = GameState::new(2, 1, LIVES_START);
        g.phase = Phase::Playing;
        g.weapons[0].slow_level = 1.0; // massimo rallentamento
        let c_before = g.players[0].c;
        g.apply_input(0, 1, 1.0 / 60.0);
        let delta_slow = (g.players[0].c - c_before).abs();

        g.weapons[0].slow_level = 0.0;
        g.players[0].c = c_before;
        g.apply_input(0, 1, 1.0 / 60.0);
        let delta_normal = (g.players[0].c - c_before).abs();

        assert!(delta_slow < delta_normal, "il rallentamento deve ridurre la velocità");
    }

    #[test]
    fn scoring_awards_grenade_to_last_hitter() {
        let mut g = GameState::new(2, 7, LIVES_START);
        g.phase = Phase::Playing;
        g.last_hitter = Some(1);
        let before = g.weapons[1].grenades;
        g.concede(0);
        assert!(g.weapons[1].grenades > before, "chi ha segnato doveva ricevere una granata");
    }
}
