//! Geometria dell'arena.
//!
//! Con 2 giocatori l'arena è un rettangolo (Pong classico): i lati corti sono
//! muri solidi, i due lati lunghi sono difesi dai giocatori. Con 3+ giocatori
//! diventa un poligono regolare a N lati e ogni giocatore difende un lato.
//!
//! Tutto è espresso come una lista omogenea di "muri" (segmenti con normale
//! rivolta verso l'interno). Un muro è solido oppure appartiene a un giocatore;
//! questo unifica completamente le due modalità e la fisica.

use crate::geom::*;

/// Circumraggio del poligono / semi-larghezza del rettangolo (unità logiche).
pub const R: f32 = 100.0;
/// Semi-altezza del rettangolo a 2 giocatori.
pub const RECT_H: f32 = 50.0;

/// Frazione del lato coperta da metà racchetta. La racchetta totale copre
/// quindi `2 * PADDLE_FRAC` della lunghezza del lato.
pub const PADDLE_FRAC: f32 = 0.14;

/// Velocità della racchetta in parametro-di-lato al secondo (l'intero lato va
/// da 0 a 1, quindi questo è "lati al secondo").
pub const PADDLE_PARAM_SPEED: f32 = 1.15;

/// Un muro dell'arena: segmento `a`→`b` con normale interna `n`.
#[derive(Clone, Copy, Debug)]
pub struct Wall {
    pub a: V2,
    pub b: V2,
    pub n: V2,
    /// `Some(pid)` se il muro è il lato difeso dal giocatore `pid`,
    /// `None` se è un muro solido fisso (lati corti del rettangolo).
    pub owner: Option<usize>,
}

impl Wall {
    /// Punto sul muro a parametro `t ∈ [0,1]`.
    #[inline]
    pub fn point(&self, t: f32) -> V2 {
        self.a + (self.b - self.a) * t
    }
}

/// L'arena completa: vertici, muri e numero di giocatori.
pub struct Arena {
    pub players: usize,
    pub walls: Vec<Wall>,
    /// Per ciascun giocatore, l'indice del proprio muro in `walls`.
    pub player_wall: Vec<usize>,
}

/// Normale interna di un segmento il cui interno contiene l'origine.
fn inward_normal(a: V2, b: V2) -> V2 {
    let mid = (a + b) * 0.5;
    // L'origine è dentro l'arena convessa centrata in 0: la normale interna
    // punta dal punto medio del lato verso il centro.
    (v2(0.0, 0.0) - mid).norm()
}

impl Arena {
    pub fn new(players: usize) -> Arena {
        if players <= 2 {
            Arena::rectangle()
        } else {
            Arena::polygon(players)
        }
    }

    /// Rettangolo per il Pong classico a 2 giocatori.
    /// Giocatore 0 = lato sinistro, giocatore 1 = lato destro.
    fn rectangle() -> Arena {
        let (w, h) = (R, RECT_H);
        let tl = v2(-w, h);
        let tr = v2(w, h);
        let br = v2(w, -h);
        let bl = v2(-w, -h);

        // Lato sinistro (giocatore 0): dal basso verso l'alto.
        let left = Wall {
            a: bl,
            b: tl,
            n: inward_normal(bl, tl),
            owner: Some(0),
        };
        // Lato destro (giocatore 1): dall'alto verso il basso.
        let right = Wall {
            a: tr,
            b: br,
            n: inward_normal(tr, br),
            owner: Some(1),
        };
        // Muri solidi sopra e sotto.
        let top = Wall {
            a: tl,
            b: tr,
            n: inward_normal(tl, tr),
            owner: None,
        };
        let bottom = Wall {
            a: br,
            b: bl,
            n: inward_normal(br, bl),
            owner: None,
        };

        Arena {
            players: 2,
            walls: vec![left, right, top, bottom],
            player_wall: vec![0, 1],
        }
    }

    /// Poligono regolare a N lati (N ≥ 3). Il lato `k` è difeso dal giocatore `k`.
    fn polygon(n: usize) -> Arena {
        // Vertici su una circonferenza di raggio R; partiamo in basso così il
        // lato del giocatore 0 sta in fondo (è comunque ri-orientato a video
        // dal punto di vista del giocatore locale).
        let mut verts = Vec::with_capacity(n);
        for k in 0..n {
            let theta = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * k as f32 / n as f32;
            verts.push(v2(R * theta.cos(), R * theta.sin()));
        }

        let mut walls = Vec::with_capacity(n);
        let mut player_wall = Vec::with_capacity(n);
        for k in 0..n {
            let a = verts[k];
            let b = verts[(k + 1) % n];
            walls.push(Wall {
                a,
                b,
                n: inward_normal(a, b),
                owner: Some(k),
            });
            player_wall.push(k);
        }

        Arena {
            players: n,
            walls,
            player_wall,
        }
    }

    /// Estremi della racchetta del giocatore `pid` dati il centro `c` e la
    /// semi-ampiezza `hw` (entrambi in parametro-di-lato). Ritorna i due punti
    /// nel mondo.
    pub fn paddle_endpoints(&self, pid: usize, c: f32, hw: f32) -> (V2, V2) {
        let w = &self.walls[self.player_wall[pid]];
        let t0 = (c - hw).clamp(0.0, 1.0);
        let t1 = (c + hw).clamp(0.0, 1.0);
        (w.point(t0), w.point(t1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_has_two_players_four_walls() {
        let ar = Arena::new(2);
        assert_eq!(ar.players, 2);
        assert_eq!(ar.walls.len(), 4);
        assert_eq!(ar.player_wall.len(), 2);
    }

    #[test]
    fn polygon_walls_match_players() {
        for n in 3..=8 {
            let ar = Arena::new(n);
            assert_eq!(ar.walls.len(), n);
            assert!(ar.walls.iter().all(|w| w.owner.is_some()));
        }
    }

    #[test]
    fn inward_normals_point_to_center() {
        let ar = Arena::new(5);
        for w in &ar.walls {
            let mid = (w.a + w.b) * 0.5;
            // muovendosi lungo la normale interna ci si avvicina all'origine
            let moved = mid + w.n * 1.0;
            assert!(moved.len() < mid.len());
        }
    }
}
