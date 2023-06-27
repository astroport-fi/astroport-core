use crate::consts::MAX_ITER;

const N: f64 = 2.0;
const TOL: f64 = 1e-5;

pub fn f(d: f64, x: &[f64], a: f64, gamma: f64) -> f64 {
    let k0 = (x[0] * x[1] * N * N) / d.powi(2);
    let k = a * gamma.powi(2) * k0 / (gamma + 1.0f64 - k0).powi(2);

    k * d * (x[0] + x[1]) + x[0] * x[1] - k * d.powi(2) - (d / N).powi(2)
}

/// df/dD
pub fn df_dd(d: f64, x: &[f64], a: f64, gamma: f64) -> f64 {
    let k0 = x[0] * x[1] * (N / d).powi(2);
    let k = a * gamma.powi(2) * k0 / (gamma + 1.0 - k0).powi(2);
    let k0_d = -x[0] * x[1] * (N / d).powi(3);
    let k_d = a * gamma.powi(2) * (gamma + 1.0 + k0) / (gamma + 1.0 - k0).powi(3) * k0_d;

    (k_d * d + k) * (x[0] + x[1]) - (k_d * d + N * k) * d - (d / N)
}

/// df/dx
pub fn df_dx(d: f64, x: &[f64], a: f64, gamma: f64, i: usize) -> f64 {
    let x_r = x[1 - i];
    let k0 = x[0] * x[1] * (N / d).powi(2);
    let k = a * gamma.powi(2) * k0 / (gamma + 1.0 - k0).powi(2);
    let k0_x = x_r * (N / d).powi(2);
    let k_x = a * gamma.powi(2) * (gamma + 1.0 + k0) / (gamma + 1.0 - k0).powi(3) * k0_x;

    (k_x * (x[0] + x[1]) + k) * d + x_r - k_x * d.powi(2)
}

pub fn newton_y(xs: &[f64], a: f64, gamma: f64, d: f64, j: usize) -> f64 {
    let mut x = xs.to_vec();
    let x_r = x[1 - j];
    let x0 = d.powi(2) / (N * N * x_r);
    let mut xi_1 = x0;
    x[j] = x0;

    println!("Computing x[{j}]. First approximation {x0}");

    let mut i = 0;
    let mut diff = 1.0;
    let mut xi = 0.0;

    while diff > TOL && i < MAX_ITER {
        xi = xi_1 - f(d, &x, a, gamma) / df_dx(d, &x, a, gamma, j);
        x[j] = xi;

        diff = (xi - xi_1).abs();
        println!("{i}, {xi}, {xi_1}");
        xi_1 = xi;
        i += 1;
    }

    xi
}

pub fn newton_d(x: &[f64], a: f64, gamma: f64) -> f64 {
    let d0 = N * (x[0] * x[1]).sqrt();
    println!("Computing D. First approximation {d0}");
    let mut di_1 = d0;
    let mut i = 0;
    let mut diff = 1.0;
    let mut di = 0.0;

    while diff > TOL && i < MAX_ITER {
        di = di_1 - f(di_1, x, a, gamma) / df_dd(di_1, x, a, gamma);
        diff = (di - di_1).abs();
        println!("{i}, {di}, {}", f(di, x, a, gamma));
        di_1 = di;
        i += 1;
    }

    di
}
