//! Processing の数学系ヘルパ。将来は VM のネイティブ関数として公開する。

/// `map()`
pub fn map(value: f32, start1: f32, stop1: f32, start2: f32, stop2: f32) -> f32 {
    if (stop1 - start1).abs() < f32::EPSILON {
        return start2;
    }
    start2 + (stop2 - start2) * ((value - start1) / (stop1 - start1))
}

/// `constrain()`
pub fn constrain(value: f32, low: f32, high: f32) -> f32 {
    value.clamp(low.min(high), high.max(low))
}

/// HSB (すべて `0.0..=1.0`) から sRGB へ。Processing の `colorMode(HSB, 1)` 相当。
pub fn hsb(h: f32, s: f32, b: f32) -> (f32, f32, f32) {
    let h = (h.fract() + 1.0).fract() * 6.0;
    let i = h.floor();
    let f = h - i;
    let p = b * (1.0 - s);
    let q = b * (1.0 - s * f);
    let t = b * (1.0 - s * (1.0 - f));
    let (r, g, bl) = match i as i32 % 6 {
        0 => (b, t, p),
        1 => (q, b, p),
        2 => (p, b, t),
        3 => (p, q, b),
        4 => (t, p, b),
        _ => (b, p, q),
    };
    (r * 255.0, g * 255.0, bl * 255.0)
}

/// `random()` 用の決定論的な乱数源。
///
/// サムネイルは実行のたびに同じ絵になってほしいので、OS の乱数ではなく
/// スケッチごとに固定シードを持つ xorshift を使う。
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// `0.0..1.0`
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }

    /// `random(high)`
    pub fn random(&mut self, high: f32) -> f32 {
        self.next_f32() * high
    }

    /// `random(low, high)`
    pub fn random_between(&mut self, low: f32, high: f32) -> f32 {
        low + self.next_f32() * (high - low)
    }
}

/// `noise(x, y)` 相当の 2D value noise。
///
/// Processing の Perlin noise と数値は一致しないが、`0.0..=1.0` に収まり
/// 連続で滑らかという性質は同じ。
pub fn noise(x: f32, y: f32) -> f32 {
    let xi = x.floor();
    let yi = y.floor();
    let xf = x - xi;
    let yf = y - yi;

    let u = smoothstep(xf);
    let v = smoothstep(yf);

    let a = hash2(xi, yi);
    let b = hash2(xi + 1.0, yi);
    let c = hash2(xi, yi + 1.0);
    let d = hash2(xi + 1.0, yi + 1.0);

    let top = a + (b - a) * u;
    let bottom = c + (d - c) * u;
    top + (bottom - top) * v
}

/// オクターブを重ねた `noise()`。輪郭が単調になりにくい。
pub fn fbm(x: f32, y: f32, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut norm = 0.0;
    for _ in 0..octaves.max(1) {
        sum += noise(x * freq, y * freq) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn hash2(x: f32, y: f32) -> f32 {
    let n = x as i32 as i64 * 374_761_393 + y as i32 as i64 * 668_265_263;
    let mut n = (n ^ (n >> 13)) as u64;
    n = n.wrapping_mul(1_274_126_177);
    ((n >> 40) as f32) / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_matches_processing() {
        assert!((map(5.0, 0.0, 10.0, 0.0, 100.0) - 50.0).abs() < 1e-4);
        assert!((map(0.0, 0.0, 10.0, 20.0, 30.0) - 20.0).abs() < 1e-4);
    }

    #[test]
    fn map_on_degenerate_range_does_not_produce_nan() {
        assert_eq!(map(5.0, 3.0, 3.0, 0.0, 100.0), 0.0);
    }

    #[test]
    fn constrain_handles_reversed_bounds() {
        assert_eq!(constrain(5.0, 10.0, 0.0), 5.0);
        assert_eq!(constrain(-1.0, 10.0, 0.0), 0.0);
    }

    #[test]
    fn noise_stays_in_unit_range_and_is_continuous() {
        let mut prev = noise(0.0, 0.0);
        for i in 0..500 {
            let n = noise(i as f32 * 0.01, 3.7);
            assert!((0.0..=1.0).contains(&n), "noise out of range: {n}");
            assert!((n - prev).abs() < 0.35, "noise jumped: {prev} -> {n}");
            prev = n;
        }
    }

    #[test]
    fn rng_is_deterministic_and_bounded() {
        let a: Vec<f32> = (0..8).map(|_| Rng::new(42).next_f32()).collect();
        let mut r = Rng::new(42);
        let b: Vec<f32> = (0..8).map(|_| r.next_f32()).collect();
        assert_eq!(a[0], b[0]);
        assert!(b.iter().all(|v| (0.0..1.0).contains(v)));
        // 同じシードから引き直せば同じ列になる。
        let mut r2 = Rng::new(42);
        assert_eq!(b, (0..8).map(|_| r2.next_f32()).collect::<Vec<_>>());
    }

    #[test]
    fn hsb_produces_valid_channels() {
        for i in 0..64 {
            let (r, g, b) = hsb(i as f32 / 64.0, 0.8, 0.9);
            for c in [r, g, b] {
                assert!((0.0..=255.0).contains(&c), "channel out of range: {c}");
            }
        }
    }
}
