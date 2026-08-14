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

/// Processing の `noise()`。
///
/// Perlin という名前で呼ばれているが、中身は乱数表を引く value noise で、
/// それを 4 オクターブ重ねたもの (`noiseDetail(4, 0.5)` が既定)。ここでは
/// Processing の実装をそのまま写している。数列が違うので模様は一致しないが、
/// 値の散らばり方と、下に挙げる癖まで揃う。
///
/// 揃えないと困る癖が 2 つある。
///
/// - **負の座標は折り返す**。`noise(-3, 0)` は `noise(3, 0)` と同じ値。
///   原点をまたいで座標を振る作品は、この折り返しのせいで左右対称になる。
///   つぶやき系はこれを承知で使っていることがある
/// - **4 オクターブ**なので値が 0.5 付近へ寄る。1 オクターブだと散らばりが
///   広く、`noise(...) > .6` のような閾値の作品で数が合わなくなる
pub fn noise(x: f32, y: f32) -> f32 {
    noise3(x, y, 0.0)
}

/// 表の大きさ。Processing と同じ。
const PERLIN_SIZE: usize = 4095;
/// y と z を表の添字へ混ぜるときのずらし幅。Processing と同じ。
const YWRAP: i32 = 1 << 4;
const ZWRAP: i32 = 1 << 8;
const OCTAVES: u32 = 4;
const AMP_FALLOFF: f32 = 0.5;

/// 乱数表。Processing は起動ごとに引き直すが、ここは決まった数列にする。
///
/// サムネイルが実行のたびに変わっては困る。
fn table() -> &'static [f32; PERLIN_SIZE + 1] {
    static TABLE: std::sync::OnceLock<[f32; PERLIN_SIZE + 1]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut rng = Rng::new(0x5EED_1234_ABCD_0001);
        std::array::from_fn(|_| rng.next_f32())
    })
}

/// 補間の重み。Processing は余弦の表を引く。
fn fsc(i: f32) -> f32 {
    0.5 * (1.0 - (i * std::f32::consts::PI).cos())
}

/// `noise(x, y, z)`。
pub fn noise3(x: f32, y: f32, z: f32) -> f32 {
    let perlin = table();
    // Processing は負の座標を折り返す。原点をまたぐ作品はここで対称になる。
    let (mut x, mut y, mut z) = (x.abs(), y.abs(), z.abs());
    // 表を大きく外れる座標は、そのまま整数へ落とすと溢れる。
    let wrap = (PERLIN_SIZE + 1) as f32;
    if x >= wrap {
        x %= wrap;
    }
    if y >= wrap {
        y %= wrap;
    }
    if z >= wrap {
        z %= wrap;
    }

    let (mut xi, mut yi, mut zi) = (x as i32, y as i32, z as i32);
    let (mut xf, mut yf, mut zf) = (x - xi as f32, y - yi as f32, z - zi as f32);

    let at = |i: i32| perlin[(i as usize) & PERLIN_SIZE];
    let mut sum = 0.0;
    let mut amp = 0.5;

    for _ in 0..OCTAVES {
        let mut of = xi + (yi * YWRAP) + (zi * ZWRAP);
        let (rxf, ryf) = (fsc(xf), fsc(yf));

        // 手前の面を x → y の順に混ぜる。
        let mut n1 = at(of);
        n1 += rxf * (at(of + 1) - n1);
        let mut n2 = at(of + YWRAP);
        n2 += rxf * (at(of + YWRAP + 1) - n2);
        n1 += ryf * (n2 - n1);

        // 奥の面も同じように。
        of += ZWRAP;
        let mut n2 = at(of);
        n2 += rxf * (at(of + 1) - n2);
        let mut n3 = at(of + YWRAP);
        n3 += rxf * (at(of + YWRAP + 1) - n3);
        n2 += ryf * (n3 - n2);

        n1 += fsc(zf) * (n2 - n1);

        sum += n1 * amp;
        amp *= AMP_FALLOFF;

        // 次のオクターブは倍の細かさで。
        xi <<= 1;
        xf *= 2.0;
        yi <<= 1;
        yf *= 2.0;
        zi <<= 1;
        zf *= 2.0;
        if xf >= 1.0 {
            xi += 1;
            xf -= 1.0;
        }
        if yf >= 1.0 {
            yi += 1;
            yf -= 1.0;
        }
        if zf >= 1.0 {
            zi += 1;
            zf -= 1.0;
        }
    }
    sum
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

    /// 負の座標は折り返す。Processing がそうしている。
    ///
    /// 原点をまたいで座標を振る作品は、これで左右対称になる。ここを直すと
    /// 「本家では対称なのにこちらは砂嵐」という差が出る。
    #[test]
    fn negative_coordinates_fold_back() {
        for (a, b) in [
            (noise3(9.3, 1.0, 6.0), noise3(-9.3, 1.0, 6.0)),
            (noise3(0.5, 2.5, 0.0), noise3(-0.5, -2.5, 0.0)),
            (noise(3.25, 0.0), noise(-3.25, 0.0)),
        ] {
            assert!((a - b).abs() < 1e-6, "{a} と {b}");
        }
    }

    /// 4 オクターブぶんの重みで、値が 0.5 のあたりへ寄る。
    ///
    /// 1 オクターブだと散らばりが広く、`noise(...) > .6` のような閾値で
    /// 数を数える作品が本家より濃くなる。
    #[test]
    fn the_spread_matches_four_octaves() {
        let mut v: Vec<f32> = (0..20_000)
            .map(|i| {
                let f = i as f32 * 0.0137;
                noise3(f, f * 0.73, f * 1.31)
            })
            .collect();
        v.sort_by(|a, b| a.partial_cmp(b).expect("NaN は出ない"));
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        assert!((mean - 0.47).abs() < 0.05, "平均が寄っていません: {mean}");
        // 0.5 + 0.25 + 0.125 + 0.0625 を超えることはない。
        assert!(v[v.len() - 1] <= 0.9375, "上限を超えました: {}", v[v.len() - 1]);
        assert!(v[0] >= 0.0, "下限を割りました: {}", v[0]);
        // 端まで振り切れない。ここが広いと 1 オクターブに戻っている。
        assert!(v[v.len() / 20] > 0.2 && v[v.len() * 19 / 20] < 0.8, "散らばりすぎです");
    }

    /// 隣り合う点は近い値になる。
    #[test]
    fn neighbouring_points_stay_close() {
        for axis in 0..3 {
            let at = |d: f32| match axis {
                0 => noise3(4.0 + d, 2.0, 1.0),
                1 => noise3(4.0, 2.0 + d, 1.0),
                _ => noise3(4.0, 2.0, 1.0 + d),
            };
            assert!((at(0.01) - at(0.0)).abs() < 0.05, "軸 {axis} が飛んでいます");
        }
    }

    /// 何度呼んでも同じ値。サムネイルが実行のたびに変わっては困る。
    #[test]
    fn the_field_is_the_same_every_run() {
        assert_eq!(noise3(1.5, 2.5, 3.5), noise3(1.5, 2.5, 3.5));
    }
}
