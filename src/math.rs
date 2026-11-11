//! Toán ma trận tối giản. Ma trận lưu theo cột: m[cột][hàng].

pub type Mat4 = [[f32; 4]; 4];

pub fn identity() -> Mat4 {
    let mut m = [[0.0; 4]; 4];
    for i in 0..4 {
        m[i][i] = 1.0;
    }
    m
}

/// r = a * b
pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut r = [[0.0; 4]; 4];
    for c in 0..4 {
        for row in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k][row] * b[c][k];
            }
            r[c][row] = s;
        }
    }
    r
}

pub fn transform(m: &Mat4, v: [f32; 4]) -> [f32; 4] {
    let mut r = [0.0; 4];
    for row in 0..4 {
        for c in 0..4 {
            r[row] += m[c][row] * v[c];
        }
    }
    r
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l == 0.0 {
        a
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}

/// Phép chiếu phối cảnh, tay phải, nhìn về -Z, độ sâu clip 0..1 (chuẩn wgpu/Metal).
pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy * 0.5).tan();
    let mut m = [[0.0; 4]; 4];
    m[0][0] = f / aspect;
    m[1][1] = f;
    m[2][2] = far / (near - far);
    m[2][3] = -1.0;
    m[3][2] = near * far / (near - far);
    m
}

/// Ma trận view: camera ở `eye`, nhìn theo hướng `dir`, trục lên là +Y.
pub fn look_to(eye: [f32; 3], dir: [f32; 3]) -> Mat4 {
    let f = normalize(dir);
    let s = normalize(cross(f, [0.0, 1.0, 0.0]));
    let u = cross(s, f);
    [
        [s[0], u[0], -f[0], 0.0],
        [s[1], u[1], -f[1], 0.0],
        [s[2], u[2], -f[2], 0.0],
        [-dot(s, eye), -dot(u, eye), dot(f, eye), 1.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_in_front_is_visible() {
        let p = perspective(1.2, 2.0, 0.1, 100.0);
        let v = look_to([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
        let c = transform(&mul(&p, &v), [0.0, 0.0, -5.0, 1.0]);
        assert!((c[3] - 5.0).abs() < 1e-4, "w = {}", c[3]);
        let z = c[2] / c[3];
        assert!(z > 0.0 && z < 1.0, "z ndc = {}", z);
        assert!(c[0].abs() < 1e-4 && c[1].abs() < 1e-4);
    }

    #[test]
    fn point_to_the_right_is_right() {
        let p = perspective(1.2, 1.0, 0.1, 100.0);
        let v = look_to([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
        let c = transform(&mul(&p, &v), [1.0, 0.0, -5.0, 1.0]);
        assert!(c[0] / c[3] > 0.0);
    }

    #[test]
    fn point_behind_is_clipped() {
        let p = perspective(1.2, 1.0, 0.1, 100.0);
        let v = look_to([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
        let c = transform(&mul(&p, &v), [0.0, 0.0, 5.0, 1.0]);
        assert!(c[3] < 0.0);
    }
}
