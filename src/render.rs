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

/// Colori distinti per i giocatori (fino a 8).
const PLAYER_COLORS: [Rgb; 8] = [
    (90, 224, 205),  // teal
    (236, 120, 196), // rosa
    (250, 196, 100), // ambra
    (130, 178, 255), // azzurro
    (160, 235, 130), // verde
    (245, 130, 120), // corallo
    (196, 150, 255), // viola
    (240, 232, 130), // giallo
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
                    let col = if pid < snap.weapons.len() {
                        let (_, slow_t, freeze_t, _) = snap.weapons[pid];
                        if freeze_t > 0.0 {
                            mix(base_col, FREEZE_TINT, 0.75)
                        } else if slow_t > 0.0 {
                            // L'intensità della tinta arancio cresce col livello di slow
                            mix(base_col, SLOW_TINT, 0.3 + 0.55 * slow_t)
                        } else {
                            base_col
                        }
                    } else {
                        base_col
                    };
                    let (e0, e1) = arena.paddle_endpoints(pid, c, PADDLE_FRAC);
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
    for &(pos, shooter) in &snap.bullets {
        let (px, py) = view.px(pos);
        let r = (BULLET_R * view.scale).max(2.0);
        f.disc(px, py, r, player_color(shooter));
    }

    // Scia + palla (nascoste a fine partita).
    if snap.phase_code != 2 {
        let n_tr = trail.len();
        for (i, &(tx, ty)) in trail.iter().enumerate() {
            let age = i as f32 / n_tr.max(1) as f32;
            let cc = mix(BG, BALL, 0.10 + 0.45 * age);
            let (px, py) = view.px(v2(tx, ty));
            f.disc(px, py, BALL_R * view.scale * 0.7, cc);
        }
        let (bx, by) = view.px(snap.ball);
        f.disc(bx, by, BALL_R * view.scale, BALL);
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
) -> String {
    let mut out = String::new();
    let dim = (120, 130, 150);
    let accent = (90, 224, 205);

    text_at(&mut out, 0, 1, accent, "▌ PONG · ARENA ▐");
    text_at(
        &mut out,
        0,
        cols.saturating_sub(title_right.chars().count() + 1),
        dim,
        title_right,
    );

    let help = "[←/→] muovi   [SPACE] spara   [G] granata   [R] rivincita   [Q] esci";
    text_at(&mut out, rows.saturating_sub(1), 1, dim, help);

    if let Some(sc) = snap {
        if my_id < sc.players.len() {
            let (_, lives, alive) = sc.players[my_id];
            let mine = if alive {
                let (ammo, _, _, grenades) =
                    sc.weapons.get(my_id).copied().unwrap_or((AMMO_MAX, 0.0, 0.0, 0));
                let bar: String = (0..AMMO_MAX as usize)
                    .map(|i| if i < ammo as usize { '█' } else { '░' })
                    .collect();
                let grenade_part = if grenades > 0 {
                    format!("  ◆x{}", grenades)
                } else {
                    String::new()
                };
                format!("{} vite  {}{}", lives.max(0), bar, grenade_part)
            } else {
                "eliminato".to_string()
            };
            let col = player_color(my_id);
            text_at(
                &mut out,
                rows.saturating_sub(1),
                cols.saturating_sub(mine.chars().count() + 1),
                col,
                &mine,
            );
        }
    }
    if let Some(st) = status {
        centered(&mut out, rows.saturating_sub(1), cols, (240, 180, 90), st);
    }
    out
}

/// Overlay di fine partita.
pub fn game_over_overlay(cols: usize, rows: usize, winner: usize, my_id: usize) -> String {
    let mut out = String::new();
    let (msg, color) = if winner == my_id {
        ("★  HAI VINTO  ★".to_string(), (120, 240, 160))
    } else {
        (
            format!("VINCE IL GIOCATORE {}", winner + 1),
            player_color(winner),
        )
    };
    let mid = rows / 2;
    centered(&mut out, mid, cols, color, &msg);
    centered(
        &mut out,
        mid + 2,
        cols,
        (160, 170, 190),
        "[R] rivincita    [Q] esci",
    );
    out
}

pub fn too_small(cols: usize, rows: usize) -> String {
    let mut out = String::from("\x1b[2J");
    centered(
        &mut out,
        rows / 2,
        cols.max(1),
        (240, 200, 120),
        "Ingrandisci il terminale (min ~54×20)",
    );
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
            let g = GameState::new(n, 1);
            let snap = g.snapshot();
            let mut f = Frame::new(80, 24);
            let trail = VecDeque::new();
            draw_arena(&mut f, &snap, 0, &trail);
        }
    }
}
