//! Rendering dell'arena su terminale (tecnica del mezzo blocco `▀`).
//!
//! Ogni giocatore vede il proprio lato in basso: ruotiamo l'intera arena in
//! base all'id locale. Disegniamo con primitive generiche (segmenti e dischi)
//! così rettangolo e poligono usano lo stesso codice.

use crate::arena::*;
use crate::game::*;
use crate::geom::*;

const SLOW_TINT: Rgb = (255, 160, 50);   // arancio per il rallentamento
const FREEZE_TINT: Rgb = (80, 190, 255); // azzurro ghiaccio per il blocco
use std::collections::VecDeque;
use std::io::{self, Write};

type Rgb = (u8, u8, u8);

const BG: Rgb = (10, 12, 18);
const WALL_SOLID: Rgb = (96, 104, 126);
const WALL_DIM: Rgb = (44, 50, 66);
const BALL: Rgb = (245, 246, 250);
const COUNT_C: Rgb = (250, 214, 120);
const LOCAL_HI: Rgb = (250, 252, 255);

/// Colori distinti per i giocatori (fino a 40).
const PLAYER_COLORS: [Rgb; 40] = [
    (90,  224, 205), // teal
    (236, 120, 196), // rosa
    (250, 196, 100), // ambra
    (130, 178, 255), // azzurro
    (160, 235, 130), // verde
    (245, 130, 120), // corallo
    (196, 150, 255), // viola
    (240, 232, 130), // giallo
    (255, 155, 190), // malva
    (140, 255, 210), // menta
    (255, 180, 100), // pesca
    (170, 215, 255), // cielo
    (210, 135, 255), // orchidea
    (255, 220, 140), // miele
    (120, 190, 120), // salvia
    (255, 105, 180), // magenta
    (110, 170, 255), // azzurro chiaro
    (220, 140, 115), // terracotta
    (180, 115, 255), // lilla
    (255, 230, 120), // crema
    (255,  80,  80), // rosso
    (180, 255,  80), // lime
    ( 80, 230, 255), // ciano
    (255, 145,  60), // arancio
    ( 60, 210, 150), // smeraldo
    (255, 175, 175), // rosa antico
    (100, 130, 255), // blu reale
    (210, 240,  80), // chartreuse
    (100, 255, 220), // acquamarina
    (255, 200, 130), // albicocca
    (120, 200, 255), // celeste
    (255,  80, 200), // fucsia
    (160, 200, 100), // oliva
    (255, 210, 160), // panna
    ( 80, 200, 200), // turchese
    (200, 180, 255), // lavanda
    (255, 240, 100), // giallo caldo
    (100, 240, 120), // verde brillante
    (255, 150, 140), // salmone
    (150, 230, 255), // acqua chiara
];

fn player_color(pid: usize) -> Rgb {
    PLAYER_COLORS[pid % PLAYER_COLORS.len()]
}

/// Font 3×5 per le cifre 0-9 (`#` = pixel acceso).
const DIGITS: [[&str; 5]; 10] = [
    ["###", "# #", "# #", "# #", "###"],
    ["  #", "  #", "  #", "  #", "  #"],
    ["###", "  #", "###", "#  ", "###"],
    ["###", "  #", "###", "  #", "###"],
    ["# #", "# #", "###", "  #", "  #"],
    ["###", "#  ", "###", "  #", "###"],
    ["###", "#  ", "###", "# #", "###"],
    ["###", "  #", "  #", "  #", "  #"],
    ["###", "# #", "###", "# #", "###"],
    ["###", "# #", "###", "  #", "###"],
];

pub struct Frame {
    pub w: usize,
    pub h: usize,
    buf: Vec<Rgb>,
}

impl Frame {
    pub fn new(cells_w: usize, cells_h: usize) -> Self {
        let w = cells_w;
        let h = cells_h * 2;
        Frame {
            w,
            h,
            buf: vec![BG; w * h],
        }
    }

    fn clear(&mut self, c: Rgb) {
        for p in self.buf.iter_mut() {
            *p = c;
        }
    }

    #[inline]
    fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.buf[y as usize * self.w + x as usize] = c;
        }
    }

    fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgb) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.set(x, y, c);
            }
        }
    }

    /// Disco pieno di raggio `r` centrato in pixel `(cx,cy)`.
    fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgb) {
        let r = r.max(0.5);
        let ri = r.ceil() as i32;
        let (cxi, cyi) = (cx.round() as i32, cy.round() as i32);
        let rr = r * r;
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                if (dx * dx + dy * dy) as f32 <= rr {
                    self.set(cxi + dx, cyi + dy, c);
                }
            }
        }
    }

    /// Segmento spesso (timbra un disco lungo il percorso).
    fn line(&mut self, p0: (f32, f32), p1: (f32, f32), r: f32, c: Rgb) {
        let dx = p1.0 - p0.0;
        let dy = p1.1 - p0.1;
        let dist = (dx * dx + dy * dy).sqrt();
        let n = (dist / 0.6).ceil() as i32;
        let n = n.max(1);
        for i in 0..=n {
            let t = i as f32 / n as f32;
            self.disc(p0.0 + dx * t, p0.1 + dy * t, r, c);
        }
    }

    fn digit(&mut self, d: usize, px: i32, py: i32, s: i32, c: Rgb) {
        for (row, line) in DIGITS[d].iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                if ch == '#' {
                    let x = px + col as i32 * s;
                    let y = py + row as i32 * s;
                    self.fill(x, y, x + s, y + s, c);
                }
            }
        }
    }

    fn number(&mut self, n: u32, center_x: i32, top_y: i32, s: i32, c: Rgb) {
        let digits: Vec<usize> = n
            .to_string()
            .chars()
            .map(|ch| ch as usize - '0' as usize)
            .collect();
        let dw = 3 * s;
        let gap = s;
        let total = digits.len() as i32 * dw + (digits.len() as i32 - 1) * gap;
        let mut x = center_x - total / 2;
        for d in digits {
            self.digit(d, x, top_y, s, c);
            x += dw + gap;
        }
    }
}

/// Trasformazione mondo → pixel, con rotazione dipendente dal giocatore locale.
struct View {
    rho: f32,
    scale: f32,
    cx: f32,
    cy: f32,
}

impl View {
    fn new(f: &Frame, n: usize, my_id: usize) -> View {
        // Il poligono ruota così che il lato locale sia in basso; il rettangolo
        // a 2 resta orizzontale (Pong classico).
        let rho = if n <= 2 {
            0.0
        } else {
            -std::f32::consts::TAU * (my_id as f32 + 0.5) / n as f32
        };
        let (hx, hy) = if n <= 2 { (R, RECT_H) } else { (R, R) };
        let margin = 1.12;
        let scale =
            (f.w as f32 / (2.0 * hx * margin)).min(f.h as f32 / (2.0 * hy * margin));
        View {
            rho,
            scale,
            cx: f.w as f32 / 2.0,
            cy: f.h as f32 / 2.0,
        }
    }

    #[inline]
    fn px(&self, p: V2) -> (f32, f32) {
        let q = p.rot(self.rho);
        (self.cx + q.x * self.scale, self.cy - q.y * self.scale)
    }
}

/// Disegna l'intera arena nel framebuffer dal punto di vista di `my_id`.
pub fn draw_arena(f: &mut Frame, snap: &Snapshot, my_id: usize, trail: &VecDeque<(f32, f32)>) {
    f.clear(BG);
    let n = snap.n;
    let arena = Arena::new(n); // geometria deterministica da n
    let view = View::new(f, n, my_id);

    // Muri e racchette.
    for w in &arena.walls {
        let pa = view.px(w.a);
        let pb = view.px(w.b);
        match w.owner {
            None => {
                // muro solido fisso (lati corti del rettangolo)
                f.line(pa, pb, 1.2, WALL_SOLID);
            }
            Some(pid) => {
                let (c, lives, alive) = snap.players[pid];
                if !alive {
                    // lato eliminato → muro solido
                    f.line(pa, pb, 1.4, WALL_SOLID);
                } else {
                    // lato difeso: linea tenue + racchetta colorata con eventuale tinta arma
                    f.line(pa, pb, 0.8, WALL_DIM);
                    let base_col = player_color(pid);
                    let (col, paddle_hw) = if pid < snap.weapons.len() {
                        let (_, slow_t, freeze_t, _, _, _, _, wide_t) = snap.weapons[pid];
                        let hw = if wide_t > 0.0 { 0.5_f32 } else { PADDLE_FRAC };
                        let c = if freeze_t > 0.0 {
                            mix(base_col, FREEZE_TINT, 0.75)
                        } else if wide_t > 0.0 {
                            mix(base_col, (50, 255, 120), 0.55)
                        } else if slow_t > 0.0 {
                            mix(base_col, SLOW_TINT, 0.3 + 0.55 * slow_t)
                        } else {
                            base_col
                        };
                        (c, hw)
                    } else {
                        (base_col, PADDLE_FRAC)
                    };
                    let (e0, e1) = arena.paddle_endpoints(pid, c, paddle_hw);
                    let (q0, q1) = (view.px(e0), view.px(e1));
                    let thick = if pid == my_id { 2.6 } else { 2.0 };
                    f.line(q0, q1, thick, col);
                    if pid == my_id {
                        // nucleo chiaro per evidenziare la propria racchetta
                        f.line(q0, q1, 1.0, LOCAL_HI);
                    }
                }
                // Vite del giocatore, appena fuori dal proprio lato.
                let mid = w.point(0.5);
                let outside = mid + (w.n * -1.0) * 7.0; // 7 unità verso l'esterno
                let (lx, ly) = view.px(outside);
                let s = ((f.h as i32) / 60).max(1);
                let col = if alive { player_color(pid) } else { WALL_DIM };
                let val = lives.max(0) as u32;
                f.number(val, lx.round() as i32, ly.round() as i32 - 2 * s, s, col);
            }
        }
    }

    // Proiettili.
    for &(pos, shooter, lethal) in &snap.bullets {
        let (px, py) = view.px(pos);
        let r = (BULLET_R * view.scale * if lethal { 2.0 } else { 1.0 }).max(2.0);
        let col = if lethal { (255, 60, 60) } else { player_color(shooter) };
        f.disc(px, py, r, col);
    }

    // Item box con anello esterno pulsante che indica l'area di raccolta.
    {
        use std::time::SystemTime;
        let ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as f32;
        let pulse = (ms * 0.005).sin() * 0.5 + 0.5; // 0..1, ~3.2 s
        for &(pos, kind) in &snap.items {
            let (ix, iy) = view.px(pos);
            let r      = (ITEM_R       * view.scale).max(4.0);
            let ring_r = (ITEM_RING_R  * view.scale).max(5.5);
            let col = item_color(kind);
            // Anello esterno (area pickup) — colore attenuato e pulsante
            let ring_col = mix(BG, col, 0.35 + 0.25 * pulse);
            f.disc(ix, iy, ring_r, ring_col);
            // Quadrato colorato al centro
            f.fill(
                (ix - r) as i32, (iy - r) as i32,
                (ix + r) as i32 + 1, (iy + r) as i32 + 1,
                col,
            );
            // Puntino bianco
            f.disc(ix, iy, (r * 0.35).max(1.5), (255, 255, 255));
        }
    }

    // Buco nero al centro dell'arena.
    if snap.black_hole_timer > 0.0 {
        use std::time::SystemTime;
        let ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as f32;
        // Pulsazione lenta (~1 Hz) + pulsazione rapida per l'alone esterno.
        let pulse_slow = (ms * 0.006).sin();          // -1..1
        let pulse_fast = (ms * 0.022).sin() * 0.5 + 0.5; // 0..1

        let (cx, cy) = view.px(v2(0.0, 0.0));
        let base = BLACK_HOLE_VIS_R * view.scale;

        // Alone esterno viola scuro (respira lentamente).
        f.disc(cx, cy, base * (1.5 + 0.18 * pulse_slow), (30, 0, 45));
        // Bordo luminoso (event horizon) che pulsa veloce.
        f.disc(cx, cy, base * (1.1 + 0.12 * pulse_fast), (160, 40, 210));
        // Nucleo quasi nero.
        f.disc(cx, cy, base * 0.72, (6, 0, 10));
        // Singolarità: puntino bianco-violaceo che batte.
        f.disc(cx, cy, base * (0.15 + 0.08 * pulse_fast), (230, 190, 255));
    }

    // Scia + palline (nascoste a fine partita).
    if snap.phase_code != 2 {
        let n_tr = trail.len();
        for (i, &(tx, ty)) in trail.iter().enumerate() {
            let age = i as f32 / n_tr.max(1) as f32;
            let cc = mix(BG, BALL, 0.10 + 0.45 * age);
            let (px, py) = view.px(v2(tx, ty));
            f.disc(px, py, BALL_R * view.scale * 0.7, cc);
        }
        for &ball_pos in &snap.balls {
            let (bx, by) = view.px(ball_pos);
            f.disc(bx, by, BALL_R * view.scale, BALL);
        }
    }

    // Conto alla rovescia gigante.
    if snap.phase_code == 0 {
        let k = snap.countdown.ceil() as u32;
        if k >= 1 {
            let s = (f.h as i32 / 9).max(3);
            f.number(k, f.w as i32 / 2, f.h as i32 / 2 - 2 * s, s, COUNT_C);
        }
    }
}

fn item_color(kind: u8) -> Rgb {
    match kind {
        0 => (255, 210, 50),  // Multiball: oro
        1 => (180, 100, 255), // Paralysis: viola
        2 => (50, 220, 220),  // Capture: acqua
        3 => (90, 0, 120),    // BlackHole: viola scuro
        4 => (255, 60, 60),   // Sniper: rosso acceso
        5 => (50, 255, 120),  // WidePaddle: verde brillante
        6 => (255, 100, 130), // ExtraLife: rosa-rosso
        _ => (200, 200, 200),
    }
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

// ---------------------------------------------------------------------------
// Riversamento del framebuffer in sequenze ANSI.
// ---------------------------------------------------------------------------
fn ansi_move(out: &mut String, row: usize, col: usize) {
    out.push_str(&format!("\x1b[{};{}H", row + 1, col + 1));
}

pub fn blit(f: &Frame, origin_col: usize, origin_row: usize) -> String {
    let mut out = String::with_capacity(f.w * f.h * 4);
    let cells_h = f.h / 2;
    let (mut cur_fg, mut cur_bg): (Option<Rgb>, Option<Rgb>) = (None, None);
    for cy in 0..cells_h {
        ansi_move(&mut out, origin_row + cy, origin_col);
        for cx in 0..f.w {
            let top = f.buf[(2 * cy) * f.w + cx];
            let bot = f.buf[(2 * cy + 1) * f.w + cx];
            if cur_fg != Some(top) {
                out.push_str(&format!("\x1b[38;2;{};{};{}m", top.0, top.1, top.2));
                cur_fg = Some(top);
            }
            if cur_bg != Some(bot) {
                out.push_str(&format!("\x1b[48;2;{};{};{}m", bot.0, bot.1, bot.2));
                cur_bg = Some(bot);
            }
            out.push('\u{2580}');
        }
    }
    out.push_str("\x1b[0m");
    out
}

// ---------------------------------------------------------------------------
// Testo di contorno.
// ---------------------------------------------------------------------------
fn item_kind_display(kind: u8) -> (&'static str, Rgb) {
    match kind {
        0 => ("⦿ MULTIBALL",  (255, 210, 50)),
        1 => ("❄ PARALISI",   (80, 190, 255)),
        2 => ("◎ CATTURA",    (160, 255, 100)),
        3 => ("☯ BUCO NERO",  (160, 100, 255)),
        4 => ("⊕ SNIPER",     (255, 150, 60)),
        5 => ("⬛ RACCHETTA", (100, 255, 160)),
        6 => ("♥ VITA+1",     (255, 100, 130)),
        _ => ("? ITEM",       (200, 200, 200)),
    }
}

fn blend_color(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    (
        (a.0 as f32 * (1.0 - t) + b.0 as f32 * t) as u8,
        (a.1 as f32 * (1.0 - t) + b.1 as f32 * t) as u8,
        (a.2 as f32 * (1.0 - t) + b.2 as f32 * t) as u8,
    )
}

fn text_at(out: &mut String, row: usize, col: usize, fg: Rgb, s: &str) {
    ansi_move(out, row, col);
    out.push_str(&format!("\x1b[38;2;{};{};{}m{}\x1b[0m", fg.0, fg.1, fg.2, s));
}

fn centered(out: &mut String, row: usize, total_cols: usize, fg: Rgb, s: &str) {
    let len = s.chars().count();
    let col = total_cols.saturating_sub(len) / 2;
    text_at(out, row, col, fg, s);
}

/// Header + footer attorno all'area di gioco.
pub fn chrome(
    cols: usize,
    rows: usize,
    title_right: &str,
    snap: Option<&Snapshot>,
    my_id: usize,
    status: Option<&str>,
    names: &[String],
) -> String {
    let mut out = String::new();
    let dim:    Rgb = (100, 108, 128);
    let accent: Rgb = (90, 224, 205);
    let sep:    Rgb = (55, 62, 82);
    let blank  = " ".repeat(cols);

    // ── Riga 0: titolo — azzerata e riscritta in un colpo solo ───────────────
    ansi_move(&mut out, 0, 0);
    out.push_str(&format!("\x1b[0m{}", blank)); // azzera la riga
    text_at(&mut out, 0, 1, accent, "▌ PONG · ARENA ▐");
    text_at(
        &mut out, 0,
        cols.saturating_sub(title_right.chars().count() + 1),
        dim, title_right,
    );

    // ── Riga 1: giocatori — azzerata prima di scrivere (evita ghost da frame precedente)
    ansi_move(&mut out, 1, 0);
    out.push_str(&format!("\x1b[0m{}", blank));
    // ── Riga 1 cont.: giocatori separati da │, si ferma prima del pannello ──
    let panel_col = cols.saturating_sub(29);
    if let Some(sc) = snap {
        let mut px = 1usize;
        for (i, ((_, lives, alive), wep)) in sc.players.iter().zip(sc.weapons.iter()).enumerate() {
            if px + 4 >= panel_col { break; }
            if i > 0 {
                text_at(&mut out, 1, px, sep, "│");
                px += 2;
            }
            let name = names.get(i).map(|s| s.as_str()).unwrap_or("?");
            let (_, _, _, grenades, _, _, _, _) = *wep;
            let marker = if i == my_id { "▸" } else { " " };
            let grens: String = (0..GRENADES_MAX as usize)
                .map(|k| if k < grenades as usize { '◆' } else { '◇' })
                .collect();
            let label = if *alive {
                format!("{}{} ♥{} {}", marker, name, (*lives).max(0), grens)
            } else {
                format!("{}{} ✗", marker, name)
            };
            let max_ch = panel_col.saturating_sub(px + 1);
            let clipped: String = label.chars().take(max_ch).collect();
            let col = if *alive { player_color(i) } else { dim };
            text_at(&mut out, 1, px, col, &clipped);
            px += clipped.chars().count() + 1;
            if px >= panel_col { break; }
        }
    }

    // ── Pannello POTERI (colonna destra, riga 2+) ───────────────────────────
    if let Some(sc) = snap {
        text_at(&mut out, 2, panel_col, dim, "── POTERI ─────────────");
        for (i, ((_, lives, alive), wep)) in sc.players.iter().zip(sc.weapons.iter()).enumerate() {
            let row = 3 + i;
            if row + 1 >= rows.saturating_sub(1) { break; }
            let (_, _, freeze_t, grenades, cap, sniper, _wounds, wide_t) = *wep;
            let name  = names.get(i).map(|s| s.as_str()).unwrap_or("?");
            let short: String = name.chars().take(7).collect();
            let marker = if i == my_id { "▸" } else { " " };
            let col_used = if *alive { player_color(i) } else { dim };
            if !*alive {
                text_at(&mut out, row, panel_col, dim,
                    &format!("{}{:<7}  ✗", marker, short));
                continue;
            }
            let grens: String = (0..GRENADES_MAX as usize)
                .map(|k| if k < grenades as usize { '◆' } else { '◇' })
                .collect();
            let mut fx = String::new();
            if wide_t > 30.0     { fx.push_str(" ⬛∞"); }
            else if wide_t > 0.0 { fx.push_str(&format!(" ⬛{:.0}s", wide_t.ceil())); }
            if sniper > 0        { fx.push_str(&format!(" ⊕{}", sniper)); }
            if freeze_t > 0.0    { fx.push_str(&format!(" ❄{:.0}s", freeze_t.ceil())); }
            if cap & 0x01 != 0   { fx.push_str(" ◎"); }
            text_at(&mut out, row, panel_col, col_used,
                &format!("{}{:<7} ♥{:<2} {}{}", marker, short, (*lives).max(0), grens, fx));
        }
    }

    // ── Footer (ultima riga): azzerata + controlli ───────────────────────────
    let last = rows.saturating_sub(1);
    ansi_move(&mut out, last, 0);
    out.push_str(&format!("\x1b[0m{}", blank));
    let help = "←/→ muovi  SPC spara  G granata  R rivincita  Q esci";
    text_at(&mut out, last, 1, dim, help);

    // ── TUO STATO: pannello ammo/vite/powerup nel lato destro ───────────────
    if let Some(sc) = snap {
        if my_id < sc.players.len() {
            let (_, lives, alive) = sc.players[my_id];
            let np = sc.players.len();
            let hud_row = 4 + np;
            if alive && hud_row + 3 < rows.saturating_sub(1) {
                let (ammo, _, _, grenades, cap, sniper, wounds, wide_t) =
                    sc.weapons.get(my_id).copied()
                        .unwrap_or((AMMO_MAX, 0.0, 0.0, 0, 0, 0, 0, 0.0));
                let col = player_color(my_id);
                text_at(&mut out, hud_row, panel_col, dim, "── TUO STATO ─────────");
                // Vite: cuori pieni/vuoti
                let hearts: String = (0..LIVES_START)
                    .map(|i| if i < lives.max(0) { "♥" } else { "·" })
                    .collect::<Vec<_>>()
                    .join(" ");
                let extra = if lives > LIVES_START {
                    format!(" +{}", lives - LIVES_START)
                } else {
                    String::new()
                };
                text_at(&mut out, hud_row + 1, panel_col, col,
                    &format!("♥  {}{}", hearts, extra));
                // Ammo (pallini) + granate
                let ammo_dots: String = (0..AMMO_MAX as usize)
                    .map(|k| if k < ammo as usize { "●" } else { "○" })
                    .collect::<Vec<_>>()
                    .join(" ");
                let reload = if ammo < AMMO_MAX { " ↺" } else { "" };
                let gren_dots: String = (0..GRENADES_MAX as usize)
                    .map(|k| if k < grenades as usize { "◆" } else { "◇" })
                    .collect::<Vec<_>>()
                    .join(" ");
                text_at(&mut out, hud_row + 2, panel_col, col,
                    &format!("⊙  {} {}  ◆ {}", ammo_dots, reload, gren_dots));
                // Powerup attivi
                let mut pows = String::new();
                if sniper > 0      { pows.push_str(&format!("⊕×{}  ", sniper)); }
                if wide_t > 30.0   { pows.push_str("⬛∞  "); }
                else if wide_t > 0.0 { pows.push_str(&format!("⬛{:.0}s  ", wide_t.ceil())); }
                if cap & 0x01 != 0 { pows.push_str("◎  "); }
                if wounds > 0      { pows.push_str(&format!("☠{}/{}", wounds, WOUND_KILLS_AT)); }
                if !pows.is_empty() {
                    text_at(&mut out, hud_row + 3, panel_col, col, &pows);
                }
            }
        }
    }
    if let Some(st) = status {
        centered(&mut out, last, cols, (240, 180, 90), st);
    }
    out
}

/// Overlay di fine partita — mostra vincitore con cornice decorativa.
pub fn game_over_overlay(cols: usize, rows: usize, winner: usize, my_id: usize, names: &[String]) -> String {
    let mut out = String::new();
    let winner_name = names.get(winner).map(|s| s.as_str()).unwrap_or("???");
    let won = winner == my_id;

    let (msg, msg_col): (String, Rgb) = if won {
        ("★  HAI VINTO  ★".to_string(), (120, 240, 160))
    } else {
        (format!("★  VINCE {}  ★", winner_name), player_color(winner))
    };

    let deco_col: Rgb = if won {
        (50, 160, 90)
    } else {
        let c = player_color(winner);
        ((c.0 / 2).max(30), (c.1 / 2).max(30), (c.2 / 2).max(30))
    };

    let deco = "·  ·  ·  ·  ·  ·  ·  ·  ·  ·";
    let mid = rows / 2;
    centered(&mut out, mid.saturating_sub(1), cols, deco_col, deco);
    centered(&mut out, mid,                   cols, msg_col,  &msg);
    centered(&mut out, mid + 1,               cols, deco_col, deco);
    centered(&mut out, mid + 3,               cols, (150, 158, 178), "R  rivincita    Q  esci");
    out
}

pub fn too_small(cols: usize, rows: usize) -> String {
    let mut out = String::from("\x1b[2J");
    let mid = rows / 2;
    centered(&mut out, mid.saturating_sub(1), cols.max(1), (90, 224, 205), "▌ PONG · ARENA ▐");
    centered(&mut out, mid,                   cols.max(1), (240, 200, 120),
        "Ingrandisci il terminale (min ~54×20)");
    centered(&mut out, mid + 1,               cols.max(1), (100, 108, 128),
        &format!("dimensione attuale: {}×{}", cols, rows));
    out
}

/// Calcola l'area di gioco. `target` è il rapporto pixel desiderato (larghezza
/// su altezza in pixel quadrati): 2.0 per il rettangolo, 1.0 per il poligono.
pub fn layout(cols: u16, rows: u16, target: f32) -> Option<(usize, usize, usize, usize)> {
    let cols = cols as usize;
    let rows = rows as usize;
    if cols < 54 || rows < 20 {
        return None;
    }
    let avail_cols = cols;
    let avail_rows = rows - 2;
    let pw = avail_cols as f32;
    let ph = (avail_rows * 2) as f32;

    let (cells_w, cells_h) = if pw / ph > target {
        let new_pw = ph * target;
        ((new_pw.round() as usize).max(2), avail_rows)
    } else {
        let new_ph = pw / target;
        (avail_cols, ((new_ph / 2.0).round() as usize).max(1))
    };

    let origin_col = (cols - cells_w) / 2;
    let origin_row = 1 + (avail_rows - cells_h) / 2;
    Some((cells_w, cells_h, origin_col, origin_row))
}

/// Overlay lampeggiante per granata. Pulisce sempre le righe bordo (or_ e rows-2)
/// per evitare che le barre rosse rimangano dopo la fine dell'effetto.
/// `oc`/`cw`: colonna e larghezza del frame di gioco, per pulire solo i margini
/// quando l'effetto non è attivo (il blit copre la parte centrale).
pub fn grenade_overlay(cols: usize, rows: usize, or_: usize, oc: usize, cw: usize, freeze_t: f32) -> String {
    use std::time::SystemTime;

    let mut out = String::new();
    let border_rows: [usize; 2] = [or_, rows.saturating_sub(2)];

    let show = freeze_t > 0.0 && {
        let ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_millis();
        (ms / 350) % 2 == 0
    };

    if show {
        let red: Rgb = (220, 55, 35);
        let bright: Rgb = (255, 140, 110);
        let bar: String = "▓".repeat(cols);
        for &tr in &border_rows {
            text_at(&mut out, tr, 0, red, &bar);
        }
        let secs = freeze_t.ceil().max(0.0) as u32;
        centered(&mut out, rows / 2, cols, bright, &format!("⚡ GRANATA  {}s ⚡", secs));
    } else {
        // Pulisci solo i margini (la parte centrale è coperta dal blit del frame di gioco)
        let left  = " ".repeat(oc);
        let right_start = oc + cw;
        let right = if cols > right_start { " ".repeat(cols - right_start) } else { String::new() };
        for &tr in &border_rows {
            if !left.is_empty()  { text_at(&mut out, tr, 0,           BG, &left);  }
            if !right.is_empty() { text_at(&mut out, tr, right_start, BG, &right); }
        }
    }

    out
}

/// Overlay ANSI con il nome di ciascun giocatore affiancato al proprio contatore vite.
/// Riga identica al bitmap del numero vite; colonna spostata a fianco del numero
/// (verso il centro del campo) così non si sovrappone alle barriere.
pub fn player_name_labels(
    snap: &Snapshot,
    names: &[String],
    cw: usize,
    ch: usize,
    oc: usize,
    or_: usize,
    my_id: usize,
    cols: usize,
    rows: usize,
) -> String {
    let n = snap.n;
    if n == 0 || names.is_empty() {
        return String::new();
    }
    let arena = Arena::new(n);

    let rho: f32 = if n <= 2 {
        0.0
    } else {
        -std::f32::consts::TAU * (my_id as f32 + 0.5) / n as f32
    };
    let (hx, hy): (f32, f32) = if n <= 2 {
        (R as f32, RECT_H as f32)
    } else {
        (R as f32, R as f32)
    };
    let margin = 1.12_f32;
    let fw = cw as f32;
    let fh = (ch * 2) as f32;
    let scale = (fw / (2.0 * hx * margin)).min(fh / (2.0 * hy * margin));
    let cx_f = fw / 2.0;
    let cy_f = fh / 2.0;
    // Stessa formula di draw_arena per il font scale del numero
    let s = (((ch * 2) as i32) / 60).max(1);

    let mut out = String::new();

    for w in &arena.walls {
        let pid = match w.owner {
            Some(p) => p,
            None => continue,
        };
        if pid >= snap.players.len() {
            continue;
        }
        let (_c, _lives, alive) = snap.players[pid];
        if !alive {
            continue;
        }

        let mid = w.point(0.5);
        let outside = mid + w.n * -7.0;
        let q = outside.rot(rho);
        let px = cx_f + q.x * scale;
        let py = cy_f - q.y * scale;

        // Stessa riga del bitmap vite (ly - 2*s in pixel → riga terminale)
        let tr = or_ as i32 + (py as i32 - 2 * s) / 2;

        if tr < 2 || tr >= rows as i32 - 1 {
            continue;
        }

        let name = names.get(pid).map(|s| s.as_str()).unwrap_or("?");
        let label: String = name.chars().take(12).collect();
        let label_len = label.chars().count() as i32;

        // Posiziona a fianco del numero: a destra se il muro è nella metà sinistra,
        // a sinistra se è nella metà destra (nome verso il centro del campo).
        let base_tc = oc as i32 + px as i32;
        let tc = if px < fw / 2.0 {
            base_tc + 3 // muro sinistro: nome a destra del numero
        } else {
            base_tc - label_len - 2 // muro destro: nome a sinistra del numero
        };
        let tc = tc.max(0).min((cols as i32 - label_len).max(0));

        let col = if pid == my_id { LOCAL_HI } else { player_color(pid) };
        out.push_str(&format!(
            "\x1b[{};{}H\x1b[38;2;{};{};{}m{}\x1b[0m",
            tr + 1,
            tc + 1,
            col.0,
            col.1,
            col.2,
            label
        ));
    }

    out
}

/// Animazione stile "cubo Mario Kart" mostrata al posto di ogni item raccolto.
/// `anims` = lista di (world_x, world_y, kind_u8, timer) dove timer scende da 0.7 a 0.0.
pub fn pickup_anim_overlay(
    anims: &[(f32, f32, u8, f32)],
    n: usize,
    my_id: usize,
    cw: usize,
    ch: usize,
    oc: usize,
    or_: usize,
) -> String {
    if anims.is_empty() {
        return String::new();
    }
    let rho: f32 = if n <= 2 {
        0.0
    } else {
        -std::f32::consts::TAU * (my_id as f32 + 0.5) / n as f32
    };
    let (hx, hy): (f32, f32) = if n <= 2 { (R as f32, RECT_H as f32) } else { (R as f32, R as f32) };
    let margin = 1.12_f32;
    let fw = cw as f32;
    let fh = (ch * 2) as f32;
    let scale = (fw / (2.0 * hx * margin)).min(fh / (2.0 * hy * margin));
    let cx_f = fw / 2.0;
    let cy_f = fh / 2.0;

    let mut out = String::new();
    const TOTAL: f32 = 0.7;

    for &(wx, wy, kind, timer) in anims {
        let q = v2(wx, wy).rot(rho);
        let px = (cx_f + q.x * scale) as usize;
        let py = (cy_f - q.y * scale) as usize;
        let tc = oc + px;
        let tr = or_ + py / 2;

        let (label, col) = item_kind_display(kind);
        let phase = (timer / TOTAL).clamp(0.0, 1.0); // 1.0 → 0.0

        if phase > 0.57 {
            // Fase 1 (0.4 s): cubo giallo con "?"
            let box_col: Rgb = (255, 218, 50);
            let tc2 = tc.saturating_sub(2);
            text_at(&mut out, tr.saturating_sub(1), tc2, box_col, "╔═══╗");
            text_at(&mut out, tr,                   tc2, box_col, "║ ? ║");
            text_at(&mut out, tr + 1,               tc2, box_col, "╚═══╝");
        } else if phase > 0.21 {
            // Fase 2 (0.25 s): icona del powerup con flash luminoso
            let t = (phase - 0.21) / 0.36;
            let flash = blend_color(col, (255, 255, 255), t * 0.6);
            text_at(&mut out, tr, tc.saturating_sub(1), flash, label);
        } else {
            // Fase 3 (0.15 s): dissolvenza verso sfondo
            let t = phase / 0.21;
            let fade = blend_color(BG, col, t);
            text_at(&mut out, tr, tc.saturating_sub(1), fade, label);
        }
    }
    out
}

/// Padding helpers Unicode-aware (chars().count(), non bytes).
fn pad_left(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.chars().take(width).collect()
    } else {
        format!("{}{}", s, " ".repeat(width - len))
    }
}

fn pad_center(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width { return s.chars().take(width).collect(); }
    let total = width - len;
    let l = total / 2;
    format!("{}{}{}", " ".repeat(l), s, " ".repeat(total - l))
}

/// Overlay di pausa / abbandono partita.
/// `is_abandon`: true = dialogo abbandono multi; false = menu pausa solo.
/// `is_host`: aggiunge avviso kick per l'host in modalità abbandono.
/// `sel`: opzione selezionata (0 = Continua/No, 1 = Esci/Sì).
pub fn pause_overlay(
    cols: usize,
    rows: usize,
    is_abandon: bool,
    is_host: bool,
    sel: usize,
) -> String {
    let mut out = String::new();
    let (title, opt0, opt1) = if is_abandon {
        (
            "  Abbandona la partita?  ",
            "No, continua",
            if is_host { "Sì — kick tutti" } else { "Sì, abbandona" },
        )
    } else {
        ("    ⏸  IN PAUSA    ", "Continua", "Esci")
    };

    let inner_w = title.chars().count()
        .max(opt0.chars().count() + 5)
        .max(opt1.chars().count() + 5)
        .max(20);
    let box_col = cols.saturating_sub(inner_w + 2) / 2;
    let mid = rows.saturating_sub(6) / 2;

    let border: Rgb = (55, 62, 82);
    let dim: Rgb    = (100, 108, 128);
    let bright: Rgb = (255, 255, 255);
    let accent: Rgb = (90, 224, 205);
    let sep = "─".repeat(inner_w);

    // Top + title
    text_at(&mut out, mid, box_col, border, &format!("╔{}╗", sep));
    text_at(&mut out, mid + 1, box_col, border, "║");
    text_at(&mut out, mid + 1, box_col + 1, accent, &pad_center(title, inner_w));
    text_at(&mut out, mid + 1, box_col + 1 + inner_w, border, "║");
    // Separator
    text_at(&mut out, mid + 2, box_col, border, &format!("╠{}╣", sep));

    // Option 0
    let l0 = format!("  {} {}", if sel == 0 { "▸" } else { " " }, opt0);
    text_at(&mut out, mid + 3, box_col, border, "║");
    text_at(&mut out, mid + 3, box_col + 1,
        if sel == 0 { bright } else { dim }, &pad_left(&l0, inner_w));
    text_at(&mut out, mid + 3, box_col + 1 + inner_w, border, "║");

    // Option 1
    let l1 = format!("  {} {}", if sel == 1 { "▸" } else { " " }, opt1);
    text_at(&mut out, mid + 4, box_col, border, "║");
    text_at(&mut out, mid + 4, box_col + 1,
        if sel == 1 { bright } else { dim }, &pad_left(&l1, inner_w));
    text_at(&mut out, mid + 4, box_col + 1 + inner_w, border, "║");

    // Bottom
    text_at(&mut out, mid + 5, box_col, border, &format!("╚{}╝", sep));

    // Hint
    centered(&mut out, mid + 7, cols, dim, "↑/↓  muovi   INVIO  conferma   ESC  chiudi");
    out
}

/// Banda spettatore mostrata nella riga di stato in basso.
pub fn spectator_overlay(cols: usize, rows: usize) -> String {
    let mut out = String::new();
    let last = rows.saturating_sub(1);
    let msg = "◎ SPETTATORE — attendi il prossimo round  |  Q  esci";
    centered(&mut out, last, cols, (90, 224, 205), msg);
    out
}

pub fn flush(s: &str) -> io::Result<()> {
    let mut out = io::stdout();
    out.write_all(s.as_bytes())?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_rejects_tiny() {
        assert!(layout(20, 10, 1.0).is_none());
    }

    #[test]
    fn layout_in_bounds() {
        let (cw, ch, oc, or_) = layout(120, 40, 1.0).expect("layout");
        assert!(cw > 0 && ch > 0);
        assert!(oc + cw <= 120);
        assert!(or_ + ch <= 40);
    }

    #[test]
    fn blit_covers_all_cells() {
        let f = Frame::new(10, 6);
        let s = blit(&f, 0, 0);
        assert_eq!(s.matches('\u{2580}').count(), 60);
    }

    #[test]
    fn draw_does_not_panic_for_many_sides() {
        for n in 2..=8 {
            let g = GameState::new(n, 1, LIVES_START);
            let snap = g.snapshot();
            let mut f = Frame::new(80, 24);
            let trail = VecDeque::new();
            draw_arena(&mut f, &snap, 0, &trail);
        }
    }
}
