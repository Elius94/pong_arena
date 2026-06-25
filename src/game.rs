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
pub const BASE_SPEED: f32 = 105.0;
pub const MAX_SPEED: f32 = 260.0;
pub const SPEEDUP: f32 = 1.055;
pub const MAX_BOUNCE: f32 = 1.0472; // ±60°
const EPS: f32 = 0.01;

// ---- Anti-stallo ----------------------------------------------------------
pub const STUCK_TIMEOUT: f32 = 10.0;  // secondi senza toccare un lato giocatore
pub const STUCK_SPAWN: usize = 10;    // palline extra da spawnare allo scadere

// ---- Progressione velocità ------------------------------------------------
pub const SPEED_RAMP_RATE: f32 = 0.3;  // incremento minSpeed per secondo
pub const SPEED_RAMP_MAX: f32 = 50.0;  // cap sopra BASE_SPEED

// ---- Armi -----------------------------------------------------------------
pub const AMMO_MAX: i32 = 3;
pub const AMMO_RELOAD_RATE: f32 = 0.1; // 30 s per ricaricare 3 colpi
pub const BULLET_SPEED: f32 = 240.0;
pub const BULLET_R: f32 = 1.2;
pub const SLOW_PER_HIT: f32 = 0.4;
pub const SLOW_DECAY_RATE: f32 = 0.40;
pub const SLOW_MIN_SPEED: f32 = 0.25;
pub const BULLET_DEFLECTION: f32 = 0.7;
pub const FREEZE_DURATION: f32 = 1.3;
pub const GRENADES_MAX: i32 = 2;
pub const BOT_JITTER: f32 = 0.10;
pub const SNIPER_AMMO: i32 = 5;    // proiettili letali per item raccolto
pub const WOUND_KILLS_AT: i32 = 3; // colpi letali per eliminare
pub const LETHAL_SPEED: f32 = 340.0;

// ---- Wide paddle powerup -------------------------------------------------
pub const WIDE_PADDLE_DURATION: f32 = 10.0;
pub const WIDE_PADDLE_HW: f32 = 0.5; // mezza-ampiezza = intero lato

// ---- Item box -------------------------------------------------------------
pub const ITEM_R: f32 = 5.0;
pub const ITEM_INTERVAL: f32 = 15.0;   // secondi tra spawn
pub const ITEM_MAX: usize = 3;
pub const PARALYSIS_DURATION: f32 = 3.0;

// ---- Buco nero ------------------------------------------------------------
pub const BLACK_HOLE_DURATION: f32 = 7.0;
pub const BLACK_HOLE_G: f32 = 5000.0;  // forza gravitazionale (unità/s²)
pub const BLACK_HOLE_VIS_R: f32 = 10.0; // raggio visuale per il rendering

// ---- Regole ---------------------------------------------------------------
pub const COUNTDOWN: f32 = 1.6;
#[allow(dead_code)]
pub const LIVES_START: i32 = 7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Countdown(f32),
    Playing,
    GameOver(usize),
}

#[derive(Clone, Debug)]
pub struct Player {
    pub c: f32,
    pub lives: i32,
    pub alive: bool,
    pub connected: bool,
    pub name: String,
}

/// Stato delle armi di un singolo giocatore.
#[derive(Clone, Debug)]
pub struct WeaponState {
    pub ammo: i32,
    pub reload_acc: f32,
    pub slow_level: f32,
    pub last_intent: i32,
    pub freeze_timer: f32,
    pub grenade_frozen: bool,
    pub grenades: i32,
    pub capture_ready: bool,
    pub sniper_ammo: i32,  // proiettili letali disponibili (da item Sniper)
    pub wound_count: i32,  // colpi letali subìti (a WOUND_KILLS_AT → eliminazione)
    pub wide_paddle_timer: f32, // secondi rimanenti di paletta larga
}

impl WeaponState {
    fn new() -> Self {
        WeaponState {
            ammo: AMMO_MAX,
            reload_acc: 0.0,
            slow_level: 0.0,
            last_intent: 0,
            freeze_timer: 0.0,
            grenade_frozen: false,
            grenades: 0,
            capture_ready: false,
            sniper_ammo: 0,
            wound_count: 0,
            wide_paddle_timer: 0.0,
        }
    }
}

/// Pallina in gioco.
pub struct Ball {
    pub pos: V2,
    pub vel: V2,
    pub captured_by: Option<usize>, // Some(pid) = trattenuta, vel = 0
}

/// Proiettile in volo.
pub struct Bullet {
    pub pos: V2,
    pub vel: V2,
    pub shooter: usize,
    pub lethal: bool, // true = proiettile Sniper; 3 colpi → eliminazione
}

/// Tipo di item box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ItemKind {
    Multiball  = 0,
    Paralysis  = 1,
    Capture    = 2,
    BlackHole  = 3,
    Sniper     = 4, // proiettili letali: 3 colpi = eliminazione istantanea
    WidePaddle = 5, // paletta larga come tutto il lato per 10 secondi
}

pub struct Item {
    pub pos: V2,
    pub kind: ItemKind,
}

// ---------------------------------------------------------------------------
// PRNG xorshift*
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

/// Stato completo della partita (vive solo lato host).
pub struct GameState {
    pub arena: Arena,
    pub balls: Vec<Ball>,
    pub phase: Phase,
    pub players: Vec<Player>,
    pub param_sign: Vec<f32>,
    pub weapons: Vec<WeaponState>,
    pub bullets: Vec<Bullet>,
    pub items: Vec<Item>,
    pub items_enabled: bool,
    pub item_spawn_timer: f32,
    pub black_hole_timer: f32,
    pub play_time: f32,
    pub kills: Vec<i32>,
    pub stuck_timer: f32, // secondi senza che la palla tocchi un lato giocatore
    last_hitter: Option<usize>,
    rng: u64,
    pub start_lives: i32,
    multiball_guard: Option<usize>, // pid che ha attivato multiball; riceve paletta larga finché ci sono palle extra
}

impl GameState {
    pub fn new(n_players: usize, seed: u64, lives: i32) -> GameState {
        let arena = Arena::new(n_players);
        let n = arena.players;
        let players = (0..n)
            .map(|_| Player {
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
            balls: Vec::new(),
            phase: Phase::Countdown(COUNTDOWN),
            players,
            param_sign,
            weapons,
            bullets: Vec::new(),
            items: Vec::new(),
            items_enabled: false,
            item_spawn_timer: 0.0,
            black_hole_timer: 0.0,
            play_time: 0.0,
            kills: vec![0; n],
            stuck_timer: 0.0,
            last_hitter: None,
            rng: seed | 1,
            start_lives: lives,
            multiball_guard: None,
        };
        g.serve();
        g
    }

    pub fn set_names(&mut self, names: &[String]) {
        for (i, p) in self.players.iter_mut().enumerate() {
            if let Some(name) = names.get(i) {
                p.name = name.clone();
            }
        }
    }

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
        if alive.len() == 1 { Some(alive[0]) } else { None }
    }

    fn current_min_speed(&self) -> f32 {
        BASE_SPEED + (self.play_time * SPEED_RAMP_RATE).min(SPEED_RAMP_MAX)
    }

    fn serve(&mut self) {
        let speed = self.current_min_speed();
        let a = rand_unit(&mut self.rng) * std::f32::consts::TAU;
        self.balls.clear();
        self.balls.push(Ball {
            pos: v2(0.0, 0.0),
            vel: v2(a.cos(), a.sin()) * speed,
            captured_by: None,
        });
        self.phase = Phase::Countdown(COUNTDOWN);
        self.bullets.clear();
        self.last_hitter = None;
    }

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
        self.items.clear();
        self.items_enabled = false;
        self.item_spawn_timer = 0.0;
        self.black_hole_timer = 0.0;
        self.play_time = 0.0;
        self.stuck_timer = 0.0;
        for k in self.kills.iter_mut() {
            *k = 0;
        }
        self.last_hitter = None;
        self.multiball_guard = None;
        self.serve();
        if let Some(w) = self.sole_survivor() {
            self.phase = Phase::GameOver(w);
        }
    }

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

    pub fn apply_action(&mut self, pid: usize, fire: bool, grenade: bool) {
        if !matches!(self.phase, Phase::Playing) {
            return;
        }
        if !self.players[pid].alive {
            return;
        }
        if fire {
            // Se il giocatore tiene una palla catturata, rilanciala.
            let held = self.balls.iter().position(|b| b.captured_by == Some(pid));
            if let Some(bi) = held {
                let wi = self.arena.player_wall[pid];
                let w = self.arena.walls[wi];
                let intent = self.weapons[pid].last_intent as f32;
                let tangent = (w.b - w.a).norm();
                let raw_dir = w.n + tangent * (intent * self.param_sign[pid] * MAX_BOUNCE);
                let speed = self.current_min_speed();
                self.balls[bi].vel = raw_dir.norm() * speed;
                self.balls[bi].captured_by = None;
                self.balls[bi].pos = w.point(self.players[pid].c) + w.n * (BALL_R * 2.0 + EPS);
                self.last_hitter = Some(pid);
            } else {
                let lethal = self.weapons[pid].sniper_ammo > 0;
                let has_ammo = lethal || self.weapons[pid].ammo > 0;
                if has_ammo {
                    if lethal {
                        self.weapons[pid].sniper_ammo -= 1;
                    } else {
                        self.weapons[pid].ammo -= 1;
                    }
                    let wi = self.arena.player_wall[pid];
                    let w = self.arena.walls[wi];
                    let c = self.players[pid].c;
                    let origin = w.point(c) + w.n * (BULLET_R + EPS);
                    let intent = self.weapons[pid].last_intent as f32;
                    let tangent = (w.b - w.a).norm();
                    let raw_dir =
                        w.n + tangent * (intent * self.param_sign[pid] * BULLET_DEFLECTION);
                    let speed = if lethal { LETHAL_SPEED } else { BULLET_SPEED };
                    self.bullets.push(Bullet {
                        pos: origin,
                        vel: raw_dir.norm() * speed,
                        shooter: pid,
                        lethal,
                    });
                }
            }
        }
        if grenade && self.weapons[pid].grenades > 0 {
            self.weapons[pid].grenades -= 1;
            for other in 0..self.players.len() {
                if other != pid && self.players[other].alive {
                    self.weapons[other].freeze_timer = FREEZE_DURATION;
                    self.weapons[other].grenade_frozen = true;
                }
            }
        }
    }

    /// Bot: decide se sparare, usare granata o rilasciare la palla trattenuta.
    /// Va chiamato ogni frame, separatamente da `bot_step`.
    pub fn bot_action(&mut self, pid: usize) {
        if !self.players[pid].alive || !matches!(self.phase, Phase::Playing) {
            return;
        }
        if self.weapons[pid].freeze_timer > 0.0 {
            return;
        }
        // Se trattiene una palla catturata, rilasciala subito mirando verso l'avversario più vicino.
        let holding = self.balls.iter().any(|b| b.captured_by == Some(pid));
        if holding {
            // Orienta l'intento verso l'avversario vivo più vicino al centro del proprio lato.
            let wi = self.arena.player_wall[pid];
            let w = self.arena.walls[wi];
            let mid = w.point(self.players[pid].c);
            let best_dir = (0..self.players.len())
                .filter(|&o| o != pid && self.players[o].alive)
                .map(|o| {
                    let ow = self.arena.player_wall[o];
                    let owall = self.arena.walls[ow];
                    let target = owall.point(self.players[o].c);
                    (target - mid).norm()
                })
                .next();
            if let Some(dir) = best_dir {
                // Converti la direzione in intent (+1/-1) lungo il lato.
                let tangent = (w.b - w.a).norm();
                let proj = dir.dot(tangent) * self.param_sign[pid];
                self.weapons[pid].last_intent = if proj > 0.1 { 1 } else if proj < -0.1 { -1 } else { 0 };
            }
            self.apply_action(pid, true, false);
            return;
        }
        // Spara con probabilità ~0.5/s (circa una volta ogni 2 secondi).
        let fire = self.weapons[pid].ammo > 0 && rand_unit(&mut self.rng) < 0.5 / 60.0;
        // Usa granata con probabilità ~0.15/s (circa una volta ogni 7 secondi).
        let grenade =
            self.weapons[pid].grenades > 0 && rand_unit(&mut self.rng) < 0.15 / 60.0;
        if fire || grenade {
            self.apply_action(pid, fire, grenade);
        }
    }

    /// Bot: insegue la palla in avvicinamento più vicina al proprio lato.
    pub fn bot_step(&mut self, pid: usize, dt: f32) {
        if !self.players[pid].alive {
            return;
        }
        if self.weapons[pid].freeze_timer > 0.0 {
            return;
        }
        let wi = self.arena.player_wall[pid];
        let w = self.arena.walls[wi];
        let mut best: Option<(f32, f32)> = None; // (projected_t, dist_to_wall)
        for ball in &self.balls {
            if ball.captured_by.is_some() {
                continue;
            }
            if ball.vel.dot(w.n) < 0.0 {
                let t = project_t(ball.pos, w.a, w.b).clamp(0.0, 1.0);
                let dist = (ball.pos - w.a).dot(w.n);
                let closer = match best {
                    None => true,
                    Some((_, d)) => dist < d,
                };
                if closer {
                    best = Some((t, dist));
                }
            }
        }
        let target = if let Some((t, _)) = best {
            if matches!(self.phase, Phase::Playing) {
                let jitter = (rand_unit(&mut self.rng) - 0.5) * 2.0 * BOT_JITTER;
                (t + jitter).clamp(PADDLE_FRAC, 1.0 - PADDLE_FRAC)
            } else {
                0.5
            }
        } else {
            0.5
        };
        let c = self.players[pid].c;
        let max_step = PADDLE_PARAM_SPEED * 0.85 * dt;
        let step = (target - c).clamp(-max_step, max_step);
        self.players[pid].c = (c + step).clamp(PADDLE_FRAC, 1.0 - PADDLE_FRAC);
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
            for ball in &mut self.balls {
                ball.vel = v2(0.0, 0.0);
            }
        }
    }

    fn eliminate(&mut self, pid: usize) {
        self.players[pid].alive = false;
        let wi = self.arena.player_wall[pid];
        self.arena.walls[wi].owner = None;
    }

    // Assegna un punto contro pid (riduce vita, elimina se necessario, controlla game over).
    // Non serve una nuova palla — farlo spetta al chiamante.
    fn award_point(&mut self, pid: usize) {
        self.stuck_timer = 0.0;
        if let Some(scorer) = self.last_hitter {
            if scorer != pid && self.players[scorer].alive {
                self.weapons[scorer].grenades =
                    (self.weapons[scorer].grenades + 1).min(GRENADES_MAX);
                if scorer < self.kills.len() {
                    self.kills[scorer] += 1;
                }
            }
        }
        if self.players[pid].lives > 0 {
            self.players[pid].lives -= 1;
        }
        // Abilita item box dopo la prima vita persa.
        if !self.items_enabled && self.players.iter().any(|p| p.lives < self.start_lives) {
            self.items_enabled = true;
            self.item_spawn_timer = ITEM_INTERVAL;
        }
        if self.players[pid].lives <= 0 && self.players[pid].alive {
            self.eliminate(pid);
            if let Some(w) = self.sole_survivor() {
                self.phase = Phase::GameOver(w);
                for ball in &mut self.balls {
                    ball.vel = v2(0.0, 0.0);
                }
            }
        }
    }

    fn concede(&mut self, pid: usize) {
        self.award_point(pid);
        if !matches!(self.phase, Phase::GameOver(_)) {
            self.serve();
        }
    }

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
                self.play_time += dt;
                self.stuck_timer += dt;
                if self.stuck_timer >= STUCK_TIMEOUT {
                    self.stuck_timer = 0.0;
                    let speed = self.current_min_speed();
                    for _ in 0..STUCK_SPAWN {
                        let a = rand_unit(&mut self.rng) * std::f32::consts::TAU;
                        self.balls.push(Ball {
                            pos: v2(0.0, 0.0),
                            vel: v2(a.cos(), a.sin()) * speed,
                            captured_by: None,
                        });
                    }
                }
                self.advance_balls(dt);
                self.tick_weapons(dt);
                self.advance_bullets(dt);
                self.tick_items(dt);
                if self.black_hole_timer > 0.0 {
                    self.black_hole_timer = (self.black_hole_timer - dt).max(0.0);
                }
            }
            Phase::GameOver(_) => {}
        }
    }

    fn advance_balls(&mut self, dt: f32) {
        let count = self.balls.len();
        // (indice palla, pid a cui viene assegnato il punto)
        let mut scored: Vec<(usize, usize)> = Vec::new();

        for bi in 0..count {
            // Palla trattenuta: segue la racchetta del giocatore.
            if let Some(holder) = self.balls[bi].captured_by {
                if self.players[holder].alive {
                    let wi = self.arena.player_wall[holder];
                    let w = self.arena.walls[wi];
                    let c = self.players[holder].c;
                    self.balls[bi].pos = w.point(c) + w.n * (BALL_R + EPS);
                }
                continue;
            }

            let speed = self.balls[bi].vel.len();
            let steps = ((speed * dt / 1.0).ceil() as i32).max(1);
            let sub = dt / steps as f32;

            'steps: for _ in 0..steps {
                // Attrazione gravitazionale del buco nero (prima dell'aggiornamento posizione).
                if self.black_hole_timer > 0.0 {
                    let to_center = v2(0.0, 0.0) - self.balls[bi].pos;
                    let dist = to_center.len().max(8.0);
                    self.balls[bi].vel = self.balls[bi].vel
                        + to_center.norm() * (BLACK_HOLE_G / dist) * sub;
                    // Limita la velocità per evitare valori estremi vicino alla singolarità.
                    let spd = self.balls[bi].vel.len();
                    if spd > MAX_SPEED * 2.0 {
                        self.balls[bi].vel = self.balls[bi].vel * (MAX_SPEED * 2.0 / spd);
                    }
                }

                self.balls[bi].pos = self.balls[bi].pos + self.balls[bi].vel * sub;

                for _iter in 0..4 {
                    let mut worst: Option<(usize, f32, f32)> = None;
                    for wi in 0..self.arena.walls.len() {
                        let w = self.arena.walls[wi];
                        let s = (self.balls[bi].pos - w.a).dot(w.n);
                        if s < BALL_R {
                            let t = project_t(self.balls[bi].pos, w.a, w.b).clamp(0.0, 1.0);
                            let replace = match worst {
                                Some((_, bs, _)) => s < bs,
                                None => true,
                            };
                            if replace {
                                worst = Some((wi, s, t));
                            }
                        }
                    }
                    let (wall_i, s, t) = match worst {
                        Some(x) => x,
                        None => break,
                    };
                    let w = self.arena.walls[wall_i];
                    match w.owner {
                        None => {
                            let vn = self.balls[bi].vel.dot(w.n);
                            self.balls[bi].vel = self.balls[bi].vel - w.n * (2.0 * vn);
                            self.balls[bi].pos =
                                self.balls[bi].pos + w.n * (BALL_R - s + EPS);
                        }
                        Some(pid) => {
                            let pc = self.players[pid].c;
                            let hw = if self.weapons[pid].wide_paddle_timer > 0.0 {
                                WIDE_PADDLE_HW
                            } else {
                                PADDLE_FRAC
                            };
                            if (t - pc).abs() <= hw {
                                // Capture item attivo: trattieni la palla.
                                if self.weapons[pid].capture_ready {
                                    self.weapons[pid].capture_ready = false;
                                    self.balls[bi].captured_by = Some(pid);
                                    self.balls[bi].vel = v2(0.0, 0.0);
                                    self.balls[bi].pos =
                                        self.balls[bi].pos + w.n * (BALL_R - s + EPS);
                                    break;
                                }
                                // Riflessione normale con speedup e ramp.
                                let off = ((t - pc) / hw).clamp(-1.0, 1.0);
                                let dir = w.n.rot(off * MAX_BOUNCE);
                                let min_speed = self.current_min_speed();
                                let new_speed = (self.balls[bi].vel.len() * SPEEDUP)
                                    .min(MAX_SPEED)
                                    .max(min_speed);
                                self.balls[bi].vel = dir * new_speed;
                                self.balls[bi].pos =
                                    self.balls[bi].pos + w.n * (BALL_R - s + EPS);
                                self.last_hitter = Some(pid);
                                self.stuck_timer = 0.0;
                            } else {
                                scored.push((bi, pid));
                                break 'steps;
                            }
                        }
                    }
                }
            }
        }

        // Ogni palla può segnare al più una volta per frame.
        scored.sort_unstable_by_key(|&(bi, _)| bi);
        scored.dedup_by_key(|&mut (bi, _)| bi);

        // Assegna i punti (senza ancora togliere le palle).
        for &(_, pid) in &scored {
            if !matches!(self.phase, Phase::GameOver(_)) {
                self.award_point(pid);
            }
        }

        // Rimuovi le palle che hanno segnato (indici in ordine inverso per stabilità).
        for &(bi, _) in scored.iter().rev() {
            if bi < self.balls.len() {
                self.balls.remove(bi);
            }
        }

        // Se le palle extra del multiball sono finite, rimuovi la protezione.
        if let Some(guard_pid) = self.multiball_guard {
            if self.balls.len() <= 1 {
                if guard_pid < self.weapons.len() {
                    self.weapons[guard_pid].wide_paddle_timer = 0.0;
                }
                self.multiball_guard = None;
            }
        }

        // Quando tutte le palle sono uscite e la partita è ancora in corso, servi.
        if self.balls.is_empty() && !matches!(self.phase, Phase::GameOver(_)) {
            self.serve();
        }
    }

    fn tick_weapons(&mut self, dt: f32) {
        for w in self.weapons.iter_mut() {
            if w.slow_level > 0.0 {
                w.slow_level = (w.slow_level - SLOW_DECAY_RATE * dt).max(0.0);
            }
            if w.freeze_timer > 0.0 {
                w.freeze_timer = (w.freeze_timer - dt).max(0.0);
                if w.freeze_timer == 0.0 {
                    w.grenade_frozen = false;
                }
            }
            if w.wide_paddle_timer > 0.0 {
                w.wide_paddle_timer = (w.wide_paddle_timer - dt).max(0.0);
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
        let mut lethal_targets: Vec<usize> = Vec::new();
        let mut to_remove: Vec<usize> = Vec::new();

        for (bi, bullet) in self.bullets.iter_mut().enumerate() {
            bullet.pos = bullet.pos + bullet.vel * dt;
            if bullet.pos.len() > R * 3.5 {
                to_remove.push(bi);
                continue;
            }
            for w in &self.arena.walls {
                let s = (bullet.pos - w.a).dot(w.n);
                if s < BULLET_R {
                    if let Some(owner) = w.owner {
                        if owner != bullet.shooter {
                            if bullet.lethal {
                                lethal_targets.push(owner);
                            } else {
                                slow_targets.push(owner);
                            }
                        }
                    }
                    to_remove.push(bi);
                    break;
                }
            }
        }

        for pid in slow_targets {
            self.weapons[pid].slow_level =
                (self.weapons[pid].slow_level + SLOW_PER_HIT).min(1.0);
        }
        let mut to_eliminate: Vec<usize> = Vec::new();
        for pid in lethal_targets {
            if self.players[pid].alive {
                self.weapons[pid].wound_count += 1;
                if self.weapons[pid].wound_count >= WOUND_KILLS_AT {
                    to_eliminate.push(pid);
                }
            }
        }
        to_eliminate.sort_unstable();
        to_eliminate.dedup();
        for pid in to_eliminate {
            self.players[pid].lives = 0;
            self.eliminate(pid);
            if let Some(w) = self.sole_survivor() {
                self.phase = Phase::GameOver(w);
            }
        }
        to_remove.sort_unstable();
        to_remove.dedup();
        for bi in to_remove.into_iter().rev() {
            self.bullets.remove(bi);
        }
    }

    fn tick_items(&mut self, dt: f32) {
        if !self.items_enabled {
            return;
        }

        // Collisione palla-item: avviene PRIMA dello spawn così un item appena
        // comparso non viene raccolto nello stesso frame in cui appare.
        let mut hits: Vec<(usize, usize)> = Vec::new(); // (item_idx, ball_idx)
        for ii in 0..self.items.len() {
            let item_pos = self.items[ii].pos;
            for bi in 0..self.balls.len() {
                if self.balls[bi].captured_by.is_some() {
                    continue;
                }
                if (self.balls[bi].pos - item_pos).len() < BALL_R + ITEM_R {
                    hits.push((ii, bi));
                    break;
                }
            }
        }
        // Ordina e de-duplica per item_idx, processa in reverse per indici stabili.
        hits.sort_by_key(|&(ii, _)| ii);
        hits.dedup_by_key(|&mut (ii, _)| ii);
        for (ii, _bi) in hits.into_iter().rev() {
            if ii >= self.items.len() {
                continue;
            }
            let item = self.items.remove(ii);
            let activator = self.last_hitter;
            match item.kind {
                ItemKind::Multiball => {
                    let speed = self.current_min_speed();
                    for _ in 0..4 {
                        let a = rand_unit(&mut self.rng) * std::f32::consts::TAU;
                        self.balls.push(Ball {
                            pos: item.pos,
                            vel: v2(a.cos(), a.sin()) * speed,
                            captured_by: None,
                        });
                    }
                    // L'attivatore riceve paletta larga per tutta la durata delle palle extra.
                    if let Some(pid) = activator {
                        self.weapons[pid].wide_paddle_timer = 99.0; // sentinella: gestita da multiball_guard
                        self.multiball_guard = Some(pid);
                    }
                }
                ItemKind::Paralysis => {
                    if let Some(pid) = activator {
                        let n = self.players.len();
                        for other in 0..n {
                            if other != pid && self.players[other].alive {
                                self.weapons[other].freeze_timer = PARALYSIS_DURATION
                                    .max(self.weapons[other].freeze_timer);
                            }
                        }
                    }
                }
                ItemKind::Capture => {
                    if let Some(pid) = activator {
                        self.weapons[pid].capture_ready = true;
                    }
                }
                ItemKind::BlackHole => {
                    self.black_hole_timer =
                        BLACK_HOLE_DURATION.max(self.black_hole_timer);
                }
                ItemKind::Sniper => {
                    if let Some(pid) = activator {
                        self.weapons[pid].sniper_ammo =
                            (self.weapons[pid].sniper_ammo + SNIPER_AMMO).min(SNIPER_AMMO * 2);
                    }
                }
                ItemKind::WidePaddle => {
                    if let Some(pid) = activator {
                        self.weapons[pid].wide_paddle_timer = WIDE_PADDLE_DURATION
                            .max(self.weapons[pid].wide_paddle_timer);
                    }
                }
            }
        }

        // Spawn timer: aggiornato DOPO la collision così il nuovo item non può
        // essere raccolto nello stesso frame. Se ITEM_MAX è pieno il timer viene
        // resettato a 0 (non scende sotto) per non far spawnare item
        // immediatamente alla prima raccolta.
        self.item_spawn_timer -= dt;
        if self.item_spawn_timer <= 0.0 {
            if self.items.len() < ITEM_MAX {
                let angle = rand_unit(&mut self.rng) * std::f32::consts::TAU;
                let dist = rand_unit(&mut self.rng).sqrt() * R * 0.4;
                let pos = v2(angle.cos() * dist, angle.sin() * dist);
                let k = xorshift(&mut self.rng) % 6;
                let kind = match k {
                    0 => ItemKind::Multiball,
                    1 => ItemKind::Paralysis,
                    2 => ItemKind::Capture,
                    3 => ItemKind::BlackHole,
                    4 => ItemKind::Sniper,
                    _ => ItemKind::WidePaddle,
                };
                self.items.push(Item { pos, kind });
                self.item_spawn_timer = ITEM_INTERVAL;
            } else {
                self.item_spawn_timer = 0.0;
            }
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let (phase_code, countdown, winner) = match self.phase {
            Phase::Countdown(t) => (0u8, t, -1i32),
            Phase::Playing => (1, 0.0, -1),
            Phase::GameOver(w) => (2, 0.0, w as i32),
        };
        let balls: Vec<V2> = self.balls.iter().map(|b| b.pos).collect();
        let weapons: Vec<(i32, f32, f32, i32, u8, i32, i32, f32)> = self
            .weapons
            .iter()
            .enumerate()
            .map(|(pid, w)| {
                // bit 0: capture_ready, bit 1: holding ball, bit 2: grenade_frozen
                let holding = self.balls.iter().any(|b| b.captured_by == Some(pid));
                let cap = (w.capture_ready as u8)
                    | ((holding as u8) << 1)
                    | ((w.grenade_frozen as u8) << 2);
                (w.ammo, w.slow_level, w.freeze_timer, w.grenades, cap, w.sniper_ammo, w.wound_count, w.wide_paddle_timer)
            })
            .collect();
        let items: Vec<(V2, u8)> = self.items.iter().map(|it| (it.pos, it.kind as u8)).collect();
        Snapshot {
            phase_code,
            countdown,
            winner,
            n: self.players.len(),
            balls,
            players: self.players.iter().map(|p| (p.c, p.lives, p.alive)).collect(),
            weapons,
            bullets: self.bullets.iter().map(|b| (b.pos, b.shooter, b.lethal)).collect(),
            items,
            black_hole_timer: self.black_hole_timer,
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
    pub balls: Vec<V2>,
    pub players: Vec<(f32, i32, bool)>,
    pub weapons: Vec<(i32, f32, f32, i32, u8, i32, i32, f32)>, // (ammo,slow,freeze,grenades,cap,sniper_ammo,wound_count,wide_paddle_timer)
    pub bullets: Vec<(V2, usize, bool)>, // (pos, shooter, lethal)
    pub items: Vec<(V2, u8)>,
    pub black_hole_timer: f32,
}

impl Snapshot {
    pub fn encode(&self) -> String {
        let mut s = format!(
            "S {} {:.3} {} {} {}",
            self.phase_code, self.countdown, self.winner, self.n, self.balls.len()
        );
        for b in &self.balls {
            s.push_str(&format!(" {:.3} {:.3}", b.x, b.y));
        }
        for (c, lives, alive) in &self.players {
            s.push_str(&format!(" {:.4} {} {}", c, lives, if *alive { 1 } else { 0 }));
        }
        for (ammo, slow, freeze, grenades, cap, sniper, wounds, wide) in &self.weapons {
            s.push_str(&format!(
                " {} {:.2} {:.2} {} {} {} {} {:.2}",
                ammo, slow, freeze, grenades, cap, sniper, wounds, wide
            ));
        }
        s.push_str(&format!(" {}", self.bullets.len()));
        for (pos, shooter, lethal) in &self.bullets {
            s.push_str(&format!(" {:.2} {:.2} {} {}", pos.x, pos.y, shooter, *lethal as u8));
        }
        s.push_str(&format!(" {}", self.items.len()));
        for (pos, kind) in &self.items {
            s.push_str(&format!(" {:.2} {:.2} {}", pos.x, pos.y, kind));
        }
        s.push_str(&format!(" {:.2}", self.black_hole_timer));
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
        let nb: usize = it.next()?.parse().ok()?;
        let mut balls = Vec::with_capacity(nb);
        for _ in 0..nb {
            let bx: f32 = it.next()?.parse().ok()?;
            let by: f32 = it.next()?.parse().ok()?;
            balls.push(v2(bx, by));
        }
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
            let cap: u8 = it.next()?.parse().ok()?;
            let sniper: i32 = it.next()?.parse().ok()?;
            let wounds: i32 = it.next()?.parse().ok()?;
            let wide: f32 = it.next()?.parse().ok()?;
            weapons.push((ammo, slow, freeze, grenades, cap, sniper, wounds, wide));
        }
        let nb_bullets: usize = it.next()?.parse().ok()?;
        let mut bullets = Vec::with_capacity(nb_bullets);
        for _ in 0..nb_bullets {
            let px: f32 = it.next()?.parse().ok()?;
            let py: f32 = it.next()?.parse().ok()?;
            let sid: usize = it.next()?.parse().ok()?;
            let lethal: u8 = it.next()?.parse().ok()?;
            bullets.push((v2(px, py), sid, lethal != 0));
        }
        let ni: usize = it.next()?.parse().ok()?;
        let mut items = Vec::with_capacity(ni);
        for _ in 0..ni {
            let ix: f32 = it.next()?.parse().ok()?;
            let iy: f32 = it.next()?.parse().ok()?;
            let kind: u8 = it.next()?.parse().ok()?;
            items.push((v2(ix, iy), kind));
        }
        let black_hole_timer: f32 = it.next()?.parse().ok()?;
        Some(Snapshot {
            phase_code,
            countdown,
            winner,
            n,
            balls,
            players,
            weapons,
            bullets,
            items,
            black_hole_timer,
        })
    }
}

// ---------------------------------------------------------------------------
// Input client → host.
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetInput {
    pub intent: i32,
    pub restart: bool,
    pub quit: bool,
    pub fire: bool,
    pub grenade: bool,
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
        assert_eq!(s.balls.len(), back.balls.len());
        assert!((s.balls[0].x - back.balls[0].x).abs() < 1e-2);
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
        assert!(g.balls[0].vel.len() > 1.0);
    }

    #[test]
    fn missing_paddle_costs_a_life() {
        let mut g = GameState::new(3, 99, LIVES_START);
        g.phase = Phase::Playing;
        let wi = g.arena.player_wall[0];
        let w = g.arena.walls[wi];
        let target = w.point(0.02);
        g.balls[0].pos = target - w.n * 1.0;
        g.balls[0].vel = w.n * (-BASE_SPEED);
        let before = g.players[0].lives;
        for _ in 0..30 {
            g.step(1.0 / 60.0);
        }
        assert!(g.players[0].lives < before, "il giocatore 0 doveva subire un punto");
    }

    #[test]
    fn elimination_closes_wall_and_can_win() {
        let mut g = GameState::new(3, 5, LIVES_START);
        g.players[2].lives = 1;
        g.concede(2);
        assert!(!g.players[2].alive);
        let wi = g.arena.player_wall[2];
        assert!(g.arena.walls[wi].owner.is_none(), "il lato deve diventare solido");
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
            assert!(
                g.balls.iter().all(|b| b.pos.len() < R * 2.0),
                "palla fuggita"
            );
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
        for _ in 0..360 {
            g.tick_weapons(1.0 / 60.0);
        }
        assert!(g.weapons[0].ammo >= 1, "doveva ricaricare almeno 1 munizione in 6 secondi");
    }

    #[test]
    fn slow_effect_reduces_paddle_speed() {
        let mut g = GameState::new(2, 1, LIVES_START);
        g.phase = Phase::Playing;
        g.weapons[0].slow_level = 1.0;
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

    #[test]
    fn speed_ramps_up_over_play_time() {
        let mut g = GameState::new(2, 1, LIVES_START);
        let speed_at_start = g.current_min_speed();
        g.play_time = 100.0; // 100 secondi simulati
        let speed_later = g.current_min_speed();
        assert!(speed_later > speed_at_start, "la velocità minima deve aumentare col tempo");
        assert!(speed_later <= BASE_SPEED + SPEED_RAMP_MAX + 0.01, "non deve superare il cap");
    }

    #[test]
    fn multiball_item_spawns_extra_balls() {
        let mut g = GameState::new(2, 1, LIVES_START);
        g.phase = Phase::Playing;
        g.items_enabled = true;
        g.last_hitter = Some(0);
        // Piazza un item Multiball dove sta la palla.
        let ball_pos = g.balls[0].pos;
        g.items.push(Item { pos: ball_pos, kind: ItemKind::Multiball });
        let before = g.balls.len();
        g.tick_items(1.0 / 60.0);
        assert!(g.balls.len() > before, "Multiball deve aggiungere palline");
    }

    #[test]
    fn capture_item_lets_player_hold_ball() {
        let mut g = GameState::new(2, 1, LIVES_START);
        g.phase = Phase::Playing;
        g.weapons[0].capture_ready = true;
        // Manda la palla contro il lato del giocatore 0 esattamente sulla racchetta.
        let wi = g.arena.player_wall[0];
        let w = g.arena.walls[wi];
        let paddle_c = g.players[0].c;
        g.balls[0].pos = w.point(paddle_c) + w.n * (BALL_R + 0.5);
        g.balls[0].vel = w.n * -BASE_SPEED;
        for _ in 0..10 {
            g.step(1.0 / 60.0);
            if g.balls[0].captured_by.is_some() {
                break;
            }
        }
        assert!(
            g.balls[0].captured_by == Some(0),
            "la palla deve essere trattenuta dal giocatore 0"
        );
    }
}
