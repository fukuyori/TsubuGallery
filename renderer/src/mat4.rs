//! 3D 用の 4x4 行列と、Processing と同じ既定カメラ。
//!
//! 変換はすべて CPU で行い、結果を画面のピクセル座標と深さに直して、2D と同じ
//! 三角形の列へ流す。GPU 側に増えるのは深度バッファだけで済む。
//!
//! この作りには限界がある。頂点色は画面空間で線形に混ざるので、遠近の効いた
//! 大きな三角形ではグラデーションが Processing とわずかにずれる。`box()` の
//! ような面ごとに 1 色の図形では違いは出ない。

/// 行優先の 4x4 行列。`m[行][列]`。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [[f32; 4]; 4]);

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    /// `self` のあとに `rhs` をローカル適用する。
    ///
    /// Processing の `translate()` / `rotate()` はこの順序で積む。
    pub fn then_local(self, rhs: Mat4) -> Mat4 {
        let mut out = [[0.0f32; 4]; 4];
        for (r, row) in out.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..4).map(|k| self.0[r][k] * rhs.0[k][c]).sum();
            }
        }
        Mat4(out)
    }

    pub fn translation(x: f32, y: f32, z: f32) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.0[0][3] = x;
        m.0[1][3] = y;
        m.0[2][3] = z;
        m
    }

    pub fn scaling(x: f32, y: f32, z: f32) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.0[0][0] = x;
        m.0[1][1] = y;
        m.0[2][2] = z;
        m
    }

    pub fn rotation_x(angle: f32) -> Mat4 {
        let (s, c) = angle.sin_cos();
        let mut m = Mat4::IDENTITY;
        m.0[1][1] = c;
        m.0[1][2] = -s;
        m.0[2][1] = s;
        m.0[2][2] = c;
        m
    }

    pub fn rotation_y(angle: f32) -> Mat4 {
        let (s, c) = angle.sin_cos();
        let mut m = Mat4::IDENTITY;
        m.0[0][0] = c;
        m.0[0][2] = s;
        m.0[2][0] = -s;
        m.0[2][2] = c;
        m
    }

    /// Z 軸まわり。2D の `rotate()` と同じ向き。
    pub fn rotation_z(angle: f32) -> Mat4 {
        let (s, c) = angle.sin_cos();
        let mut m = Mat4::IDENTITY;
        m.0[0][0] = c;
        m.0[0][1] = -s;
        m.0[1][0] = s;
        m.0[1][1] = c;
        m
    }

    /// 任意の軸まわり。`rotate(angle, x, y, z)` 用。軸は正規化する。
    pub fn rotation_axis(angle: f32, x: f32, y: f32, z: f32) -> Mat4 {
        let len = (x * x + y * y + z * z).sqrt();
        if len < 1e-6 {
            return Mat4::IDENTITY;
        }
        let (x, y, z) = (x / len, y / len, z / len);
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        Mat4([
            [t * x * x + c, t * x * y - s * z, t * x * z + s * y, 0.0],
            [t * x * y + s * z, t * y * y + c, t * y * z - s * x, 0.0],
            [t * x * z - s * y, t * y * z + s * x, t * z * z + c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// 点を移す。平行移動も効く。
    pub fn point(&self, p: [f32; 3]) -> [f32; 3] {
        let m = &self.0;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3],
        ]
    }

    /// 向きを移す。平行移動は効かない。
    ///
    /// 法線には本来は逆行列の転置が要る。回転と一様な拡大だけなら左上 3x3 で
    /// 合うので、そちらで済ませている。軸ごとに違う倍率をかけた作品では
    /// 陰影がわずかにずれる。
    pub fn direction(&self, v: [f32; 3]) -> [f32; 3] {
        let m = &self.0;
        [
            m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
            m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
            m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        ]
    }
}

/// 単位ベクトルに直す。長さが 0 なら Z 軸を返す。
pub fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-6 { [0.0, 0.0, 1.0] } else { [v[0] / len, v[1] / len, v[2] / len] }
}

/// `size(w, h, P3D)` / `createCanvas(w, h, WEBGL)` の違い。
///
/// 原点の位置だけが違う。Processing は画面の左上、p5.js の WEBGL は中央。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    TopLeft,
    Center,
}

/// 3D の視点。Processing の既定カメラと同じ値を持つ。
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// 視野角 (ラジアン)。既定は 60 度。
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    /// キャンバスの縦横比。
    pub aspect: f32,
    /// キャンバスの大きさ。画面座標へ直すのに使う。
    pub size: (f32, f32),
    pub origin: Origin,
}

impl Camera {
    /// Processing / p5.js の既定値。
    ///
    /// 視野角 60 度、視点は `z = (height/2) / tan(30°)`。この距離だと
    /// `z = 0` の平面がちょうど 1 ピクセル 1 単位で写る。
    pub fn new(width: f32, height: f32, origin: Origin) -> Camera {
        let fov = std::f32::consts::FRAC_PI_3;
        let eye_z = (height * 0.5) / (fov * 0.5).tan();
        Camera {
            fov,
            near: eye_z * 0.1,
            far: eye_z * 10.0,
            aspect: if height > 0.0 { width / height } else { 1.0 },
            size: (width, height),
            origin,
        }
    }

    /// 視点から `z = 0` の平面までの距離。
    pub fn eye_z(&self) -> f32 {
        (self.size.1 * 0.5) / (self.fov * 0.5).tan()
    }

    /// フレームの始めのモデルビュー行列。
    ///
    /// Processing の `resetMatrix()` はこれを単位行列へ戻す。視点が原点に移り、
    /// 画面の中心が -Z 方向になるので、原点まわりに置いた立体がそのまま
    /// 中央に写る。つぶやき系の作品はこれをよく使う。
    pub fn modelview(&self) -> Mat4 {
        match self.origin {
            Origin::TopLeft => {
                Mat4::translation(-self.size.0 * 0.5, -self.size.1 * 0.5, -self.eye_z())
            }
            Origin::Center => Mat4::translation(0.0, 0.0, -self.eye_z()),
        }
    }

    /// 視点座標の 1 点をキャンバスのピクセル座標と深さへ直す。
    ///
    /// 深さは 0 が near、1 が far。視点より手前 (near より近い) の点は
    /// 写せないので `None` を返す。
    pub fn project(&self, eye: [f32; 3]) -> Option<([f32; 2], f32)> {
        // 視点は原点にあり -Z 方向を見ている。距離は -z。
        let distance = -eye[2];
        // NaN もここで落とす。1 点でも壊れると面が画面いっぱいに伸びる。
        if distance <= self.near || distance.is_nan() {
            return None;
        }
        let half = (self.fov * 0.5).tan();
        let ndc_x = eye[0] / (distance * half * self.aspect);
        // Y は下向き。2D の座標系と揃える。
        let ndc_y = eye[1] / (distance * half);
        let depth = self.far * (distance - self.near) / ((self.far - self.near) * distance);
        Some((
            [(ndc_x * 0.5 + 0.5) * self.size.0, (ndc_y * 0.5 + 0.5) * self.size.1],
            depth,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-3, "{a} != {b}");
    }

    #[test]
    fn the_identity_leaves_a_point_alone() {
        assert_eq!(Mat4::IDENTITY.point([1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
    }

    /// 積む順序が Processing と同じか。
    ///
    /// `translate(10,0,0); rotateZ(PI/2);` なら、まず回してから動かす、ではなく
    /// 動かした先で回す。
    #[test]
    fn transforms_stack_in_the_order_they_are_written() {
        let m = Mat4::IDENTITY
            .then_local(Mat4::translation(10.0, 0.0, 0.0))
            .then_local(Mat4::rotation_z(std::f32::consts::FRAC_PI_2));
        let p = m.point([1.0, 0.0, 0.0]);
        approx(p[0], 10.0);
        approx(p[1], 1.0);
    }

    #[test]
    fn each_axis_turns_the_right_way() {
        let q = std::f32::consts::FRAC_PI_2;
        let x = Mat4::rotation_x(q).point([0.0, 1.0, 0.0]);
        approx(x[1], 0.0);
        approx(x[2], 1.0);
        let y = Mat4::rotation_y(q).point([0.0, 0.0, 1.0]);
        approx(y[0], 1.0);
        // 任意軸の回転は、その軸まわりの専用の式と一致する。
        let a = Mat4::rotation_axis(0.7, 0.0, 0.0, 1.0).point([1.0, 2.0, 3.0]);
        let b = Mat4::rotation_z(0.7).point([1.0, 2.0, 3.0]);
        for i in 0..3 {
            approx(a[i], b[i]);
        }
    }

    /// 既定のカメラでは `z = 0` の平面が 1 ピクセル 1 単位で写る。
    ///
    /// P3D で `rect()` を書いた作品が 2D と同じ場所に出る、ということ。
    #[test]
    fn the_default_camera_maps_the_zero_plane_one_to_one() {
        let cam = Camera::new(720.0, 720.0, Origin::TopLeft);
        let view = cam.modelview();
        for (world, screen) in [
            ([0.0, 0.0, 0.0], [0.0, 0.0]),
            ([720.0, 720.0, 0.0], [720.0, 720.0]),
            ([360.0, 360.0, 0.0], [360.0, 360.0]),
            ([100.0, 620.0, 0.0], [100.0, 620.0]),
        ] {
            let (at, _) = cam.project(view.point(world)).expect("写る");
            approx(at[0], screen[0]);
            approx(at[1], screen[1]);
        }
    }

    /// `resetMatrix()` のあとは原点まわりが画面の中央に来る。
    #[test]
    fn resetting_the_matrix_puts_the_origin_at_the_centre() {
        let cam = Camera::new(720.0, 720.0, Origin::TopLeft);
        let (at, _) = cam.project(Mat4::IDENTITY.point([0.0, 0.0, -130.0])).expect("写る");
        approx(at[0], 360.0);
        approx(at[1], 360.0);
    }

    /// 遠いものほど深さが大きい。手前すぎるものは写さない。
    #[test]
    fn depth_grows_with_distance() {
        let cam = Camera::new(720.0, 720.0, Origin::TopLeft);
        let (_, near) = cam.project([0.0, 0.0, -100.0]).unwrap();
        let (_, far) = cam.project([0.0, 0.0, -500.0]).unwrap();
        assert!((0.0..=1.0).contains(&near) && near < far, "{near} < {far}");
        assert!(cam.project([0.0, 0.0, -1.0]).is_none());
        assert!(cam.project([0.0, 0.0, 10.0]).is_none());
    }

    /// p5.js の WEBGL は原点が画面の中央。
    #[test]
    fn webgl_puts_the_origin_at_the_centre() {
        let cam = Camera::new(400.0, 400.0, Origin::Center);
        let (at, _) = cam.project(cam.modelview().point([0.0, 0.0, 0.0])).expect("写る");
        approx(at[0], 200.0);
        approx(at[1], 200.0);
    }
}
