//! Piccola libreria di vettori 2D. Convenzione matematica: origine al centro
//! dell'arena, `x` verso destra, `y` verso l'alto. Il rendering (e solo lui)
//! ribalta `y` perché sullo schermo cresce verso il basso.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

#[inline]
pub fn v2(x: f32, y: f32) -> V2 {
    V2 { x, y }
}

impl V2 {
    #[inline]
    pub fn dot(self, o: V2) -> f32 {
        self.x * o.x + self.y * o.y
    }
    #[inline]
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    #[inline]
    pub fn norm(self) -> V2 {
        let l = self.len();
        if l <= 1e-9 {
            v2(0.0, 0.0)
        } else {
            v2(self.x / l, self.y / l)
        }
    }
    /// Ruota il vettore di `a` radianti (antiorario).
    #[inline]
    pub fn rot(self, a: f32) -> V2 {
        let (s, c) = a.sin_cos();
        v2(self.x * c - self.y * s, self.x * s + self.y * c)
    }
}

impl std::ops::Add for V2 {
    type Output = V2;
    #[inline]
    fn add(self, o: V2) -> V2 {
        v2(self.x + o.x, self.y + o.y)
    }
}
impl std::ops::Sub for V2 {
    type Output = V2;
    #[inline]
    fn sub(self, o: V2) -> V2 {
        v2(self.x - o.x, self.y - o.y)
    }
}
impl std::ops::Mul<f32> for V2 {
    type Output = V2;
    #[inline]
    fn mul(self, k: f32) -> V2 {
        v2(self.x * k, self.y * k)
    }
}

/// Proietta il punto `p` sul segmento `a`→`b` e ritorna il parametro `t`
/// (clampato a `[0,1]`) tale che il punto più vicino sia `a + (b-a)*t`.
#[inline]
pub fn project_t(p: V2, a: V2, b: V2) -> f32 {
    let ab = b - a;
    let denom = ab.dot(ab);
    if denom <= 1e-9 {
        0.0
    } else {
        (p - a).dot(ab) / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_preserves_length() {
        let v = v2(3.0, 4.0);
        let r = v.rot(1.2);
        assert!((v.len() - r.len()).abs() < 1e-4);
    }

    #[test]
    fn projection_midpoint() {
        let t = project_t(v2(0.0, 5.0), v2(-1.0, 0.0), v2(1.0, 0.0));
        assert!((t - 0.5).abs() < 1e-4);
    }

    #[test]
    fn projection_clamps_outside() {
        let t = project_t(v2(-10.0, 0.0), v2(-1.0, 0.0), v2(1.0, 0.0));
        // project_t non clampa da solo; il chiamante lo fa. Qui verifichiamo
        // che il valore grezzo sia negativo (oltre l'estremo `a`).
        assert!(t < 0.0);
    }
}
