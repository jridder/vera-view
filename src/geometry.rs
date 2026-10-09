//! 2-D math helpers and curve flattening for Visio geometry rows.

use std::f64::consts::{PI, TAU};

pub type P = [f64; 2];

/// Affine transform mapping (x, y) to (a·x + c·y + e, b·x + d·y + f).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Self = Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translate(x: f64, y: f64) -> Self {
        Self { e: x, f: y, ..Self::IDENTITY }
    }

    pub fn scale(sx: f64, sy: f64) -> Self {
        Self { a: sx, d: sy, ..Self::IDENTITY }
    }

    pub fn rotate(angle: f64) -> Self {
        let (s, c) = angle.sin_cos();
        Self { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 }
    }

    /// `self ∘ other`: apply `other` first, then `self`.
    pub fn then_after(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }

    pub fn apply(&self, p: P) -> P {
        [self.a * p[0] + self.c * p[1] + self.e, self.b * p[0] + self.d * p[1] + self.f]
    }

    pub fn apply_vec(&self, v: P) -> P {
        [self.a * v[0] + self.c * v[1], self.b * v[0] + self.d * v[1]]
    }
}

pub fn dist(a: P, b: P) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn lerp(a: P, b: P, t: f64) -> P {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn segments_for_sweep(sweep: f64) -> usize {
    ((sweep.abs() / TAU) * 128.0).ceil().clamp(4.0, 256.0) as usize
}

/// Circular arc from `p0` through `pm` to `p1`. Pushes points after `p0`.
pub fn arc_through(p0: P, pm: P, p1: P, out: &mut Vec<P>) {
    let (ax, ay) = (p0[0], p0[1]);
    let (bx, by) = (pm[0], pm[1]);
    let (cx, cy) = (p1[0], p1[1]);
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    let scale = dist(p0, p1).max(dist(p0, pm)).max(1e-9);
    if d.abs() < 1e-9 * scale * scale {
        out.push(p1);
        return;
    }
    let a2 = ax * ax + ay * ay;
    let b2 = bx * bx + by * by;
    let c2 = cx * cx + cy * cy;
    let ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
    let uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
    let r = dist([ux, uy], p0);
    let t0 = (ay - uy).atan2(ax - ux);
    let tm = (by - uy).atan2(bx - ux);
    let t1 = (cy - uy).atan2(cx - ux);
    // Counter-clockwise sweep from t0 to t1; flip direction if pm is not on it.
    let ccw = |from: f64, to: f64| (to - from).rem_euclid(TAU);
    let mut sweep = ccw(t0, t1);
    if ccw(t0, tm) > sweep {
        sweep -= TAU;
    }
    let n = segments_for_sweep(sweep);
    for i in 1..n {
        let t = t0 + sweep * i as f64 / n as f64;
        out.push([ux + r * t.cos(), uy + r * t.sin()]);
    }
    out.push(p1);
}

/// Visio `ArcTo`: circular arc to `p1` whose midpoint is `bow` away from the chord.
pub fn arc_to(p0: P, p1: P, bow: f64, out: &mut Vec<P>) {
    let len = dist(p0, p1);
    if bow.abs() < 1e-9 || len < 1e-12 {
        out.push(p1);
        return;
    }
    let mid = lerp(p0, p1, 0.5);
    // Positive bow bulges to the right of the direction of travel.
    let n = [(p1[1] - p0[1]) / len, -(p1[0] - p0[0]) / len];
    arc_through(p0, [mid[0] + n[0] * bow, mid[1] + n[1] * bow], p1, out);
}

/// Visio `EllipticalArcTo`: elliptical arc to `p1` passing through `ctrl`, with the
/// major axis at `angle` and `ratio` = major / minor axis length.
pub fn elliptical_arc_to(p0: P, ctrl: P, p1: P, angle: f64, ratio: f64, out: &mut Vec<P>) {
    let ratio = if ratio.is_finite() && ratio.abs() > 1e-9 { ratio } else { 1.0 };
    // Map the ellipse to a circle, solve there, and map back.
    let to_circle = Affine::scale(1.0 / ratio, 1.0).then_after(&Affine::rotate(-angle));
    let from_circle = Affine::rotate(angle).then_after(&Affine::scale(ratio, 1.0));
    let mut pts = Vec::new();
    arc_through(to_circle.apply(p0), to_circle.apply(ctrl), to_circle.apply(p1), &mut pts);
    let last = pts.len() - 1;
    out.extend(pts[..last].iter().map(|&p| from_circle.apply(p)));
    out.push(p1);
}

/// Visio `Ellipse` row: centre, end of one axis, end of the other axis.
pub fn ellipse(center: P, a: P, b: P) -> Vec<P> {
    let u = [a[0] - center[0], a[1] - center[1]];
    let v = [b[0] - center[0], b[1] - center[1]];
    let n = 128;
    (0..=n)
        .map(|i| {
            let t = TAU * i as f64 / n as f64;
            let (s, c) = t.sin_cos();
            [center[0] + u[0] * c + v[0] * s, center[1] + u[1] * c + v[1] * s]
        })
        .collect()
}

pub fn cubic_to(p0: P, c1: P, c2: P, p1: P, out: &mut Vec<P>) {
    let n = 32;
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let mt = 1.0 - t;
        let w = [mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t];
        out.push([
            w[0] * p0[0] + w[1] * c1[0] + w[2] * c2[0] + w[3] * p1[0],
            w[0] * p0[1] + w[1] * c1[1] + w[2] * c2[1] + w[3] * p1[1],
        ]);
    }
}

pub fn quad_to(p0: P, c: P, p1: P, out: &mut Vec<P>) {
    let n = 24;
    for i in 1..=n {
        let t = i as f64 / n as f64;
        out.push(lerp(lerp(p0, c, t), lerp(c, p1, t), t));
    }
}

/// Evaluate a (rational) B-spline and push sample points after the first control point.
pub fn nurbs(ctrl: &[P], weights: &[f64], knots: &[f64], degree: usize, out: &mut Vec<P>) {
    let n = ctrl.len();
    if n < 2 || degree == 0 {
        out.extend_from_slice(ctrl.get(1..).unwrap_or_default());
        return;
    }
    let degree = degree.min(n - 1);
    let mut knots = knots.to_vec();
    while knots.len() < n + degree + 1 {
        knots.push(knots.last().copied().unwrap_or(0.0));
    }
    knots.truncate(n + degree + 1);
    let (t_start, t_end) = (knots[degree], knots[n]);
    if t_end - t_start <= 1e-12 {
        out.extend_from_slice(&ctrl[1..]);
        return;
    }
    let weight = |i: usize| weights.get(i).copied().filter(|w| *w > 0.0).unwrap_or(1.0);
    let samples = 24 * (n - degree).max(1);
    for s in 1..=samples {
        let t = t_start + (t_end - t_start) * s as f64 / samples as f64;
        // Find the knot span containing t.
        let mut k = degree;
        while k + 1 < n && knots[k + 1] <= t {
            k += 1;
        }
        // de Boor's algorithm in homogeneous coordinates.
        let mut d: Vec<[f64; 3]> = (0..=degree)
            .map(|j| {
                let i = k + j - degree;
                let w = weight(i);
                [ctrl[i][0] * w, ctrl[i][1] * w, w]
            })
            .collect();
        for r in 1..=degree {
            for j in (r..=degree).rev() {
                let i = k + j - degree;
                let denom = knots[i + degree + 1 - r] - knots[i];
                let alpha = if denom.abs() < 1e-12 { 0.0 } else { (t - knots[i]) / denom };
                let prev = d[j - 1];
                for (cur, prev) in d[j].iter_mut().zip(prev) {
                    *cur = (1.0 - alpha) * prev + alpha * *cur;
                }
            }
        }
        let [x, y, w] = d[degree];
        if w.abs() > 1e-12 {
            out.push([x / w, y / w]);
        }
    }
    if let (Some(last), Some(end)) = (out.last_mut(), ctrl.last()) {
        *last = *end;
    }
}

/// Angle of `v`, normalised to (-π, π].
pub fn angle_of(v: P) -> f64 {
    let a = v[1].atan2(v[0]);
    if a <= -PI { a + TAU } else { a }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: P, b: P) -> bool {
        dist(a, b) < 1e-6
    }

    #[test]
    fn arc_to_bulges_right_for_positive_bow() {
        let mut pts = Vec::new();
        arc_to([0.0, 0.0], [2.0, 0.0], 1.0, &mut pts);
        let lowest = pts.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        assert!((lowest + 1.0).abs() < 1e-3, "lowest = {lowest}");
        assert!(close(*pts.last().unwrap(), [2.0, 0.0]));
    }

    #[test]
    fn elliptical_arc_passes_through_control() {
        let mut pts = Vec::new();
        elliptical_arc_to([0.0, 0.0], [1.0, 0.5], [2.0, 0.0], 0.0, 2.0, &mut pts);
        assert!(pts.iter().any(|&p| dist(p, [1.0, 0.5]) < 0.02));
    }

    #[test]
    fn clamped_nurbs_hits_endpoints() {
        let ctrl = [[0.0, 0.0], [1.0, 2.0], [2.0, 2.0], [3.0, 0.0]];
        let mut pts = Vec::new();
        nurbs(&ctrl, &[1.0; 4], &[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0], 3, &mut pts);
        assert!(close(*pts.last().unwrap(), [3.0, 0.0]));
        // 24 samples at t = 1/24 ..= 1, so index 11 is t = 0.5.
        assert!(close(pts[11], [1.5, 1.5]));
    }

    #[test]
    fn affine_composes_in_order() {
        let t = Affine::translate(1.0, 0.0).then_after(&Affine::rotate(PI / 2.0));
        assert!(close(t.apply([1.0, 0.0]), [1.0, 1.0]));
    }
}
