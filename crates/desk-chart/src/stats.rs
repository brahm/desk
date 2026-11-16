// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The arithmetic of trendlines and error bars (GRF-13, GRF-14).

use crate::model::{ErrorAmount, TrendKind};

/// A fitted trendline.
#[derive(Clone, Debug, PartialEq)]
pub struct Fit {
    /// The kind.
    pub kind: TrendKind,
    /// The coefficients: a and b of the line, the logarithm, the power
    /// and the exponential; the polynomial from the constant up.
    pub coefficients: Vec<f64>,
    /// How well it fits, from 0 to 1; none for a moving average.
    pub r_squared: Option<f64>,
}

impl Fit {
    /// The value of the line at x.
    pub fn at(&self, x: f64) -> f64 {
        let c = &self.coefficients;
        match self.kind {
            TrendKind::Linear => c[0] + c[1] * x,
            TrendKind::Logarithmic => c[0] + c[1] * x.ln(),
            TrendKind::Polynomial(_) => c.iter().rev().fold(0.0, |acc, k| acc * x + k),
            TrendKind::Power => c[0] * x.powf(c[1]),
            TrendKind::Exponential => c[0] * (c[1] * x).exp(),
            TrendKind::MovingAverage(_) => f64::NAN,
        }
    }

    /// The equation, as the chart shows it: y = 2,5x + 1,2.
    pub fn equation(&self, decimal: char) -> String {
        let n = |v: f64| {
            // Round first, so that -0.00001 is written 0.
            let v = (v * 1e4).round() / 1e4 + 0.0;
            let s = format!("{:.4}", v);
            let s = s.trim_end_matches('0').trim_end_matches('.').to_owned();
            s.replace('.', &decimal.to_string())
        };
        let signed = |v: f64| {
            if v < 0.0 {
                format!(" − {}", n(-v))
            } else {
                format!(" + {}", n(v))
            }
        };
        let c = &self.coefficients;
        match self.kind {
            TrendKind::Linear => format!("y = {}x{}", n(c[1]), signed(c[0])),
            TrendKind::Logarithmic => format!("y = {} ln(x){}", n(c[1]), signed(c[0])),
            TrendKind::Polynomial(_) => {
                let mut s = String::from("y =");
                for (p, k) in c.iter().enumerate().rev() {
                    let term = match p {
                        0 => String::new(),
                        1 => "x".to_owned(),
                        _ => format!("x{}", superscript(p)),
                    };
                    if p == c.len() - 1 {
                        s += &format!(" {}{term}", n(*k));
                    } else {
                        s += &format!("{}{term}", signed(*k));
                    }
                }
                s
            }
            TrendKind::Power => format!("y = {}x{}", n(c[0]), superscript_text(&n(c[1]))),
            TrendKind::Exponential => format!(
                "y = {}e{}",
                n(c[0]),
                superscript_text(&format!("{}x", n(c[1])))
            ),
            TrendKind::MovingAverage(_) => String::new(),
        }
    }
}

fn superscript(p: usize) -> String {
    superscript_text(&p.to_string())
}

fn superscript_text(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            '-' | '−' => '⁻',
            ',' | '.' => '·',
            'x' => 'ˣ',
            other => other,
        })
        .collect()
}

/// Solves a linear system by Gaussian elimination; none when it has no
/// single solution.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|i, j| a[*i][col].abs().total_cmp(&a[*j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..n {
            let f = a[row][col] / a[col][col];
            for k in col..n {
                a[row][k] -= f * a[col][k];
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|k| a[i][k] * x[k]).sum();
        x[i] = (b[i] - s) / a[i][i];
    }
    Some(x)
}

/// The least-squares polynomial of an order through points.
#[allow(clippy::needless_range_loop)]
fn polynomial(xs: &[f64], ys: &[f64], order: usize) -> Option<Vec<f64>> {
    let m = order + 1;
    let mut a = vec![vec![0.0; m]; m];
    let mut b = vec![0.0; m];
    for (x, y) in xs.iter().zip(ys) {
        for i in 0..m {
            for j in 0..m {
                a[i][j] += x.powi((i + j) as i32);
            }
            b[i] += y * x.powi(i as i32);
        }
    }
    solve(a, b)
}

fn r_squared(xs: &[f64], ys: &[f64], f: impl Fn(f64) -> f64) -> f64 {
    let mean = ys.iter().sum::<f64>() / ys.len() as f64;
    let total: f64 = ys.iter().map(|y| (y - mean).powi(2)).sum();
    let residual: f64 = xs.iter().zip(ys).map(|(x, y)| (y - f(*x)).powi(2)).sum();
    if total == 0.0 {
        1.0
    } else {
        1.0 - residual / total
    }
}

/// Fits a trendline through points; none when the points do not allow
/// it, as a logarithm of values not above zero (GRF-14).
pub fn fit(kind: TrendKind, xs: &[f64], ys: &[f64]) -> Option<Fit> {
    if xs.len() != ys.len() || xs.len() < 2 {
        return None;
    }
    let coefficients = match kind {
        TrendKind::Linear => polynomial(xs, ys, 1)?,
        TrendKind::Polynomial(order) => {
            let order = usize::from(order.clamp(2, 6)).min(xs.len() - 1);
            polynomial(xs, ys, order)?
        }
        TrendKind::Logarithmic => {
            if xs.iter().any(|x| *x <= 0.0) {
                return None;
            }
            let lx: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
            polynomial(&lx, ys, 1)?
        }
        TrendKind::Power => {
            if xs.iter().chain(ys).any(|v| *v <= 0.0) {
                return None;
            }
            let lx: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
            let ly: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
            let c = polynomial(&lx, &ly, 1)?;
            vec![c[0].exp(), c[1]]
        }
        TrendKind::Exponential => {
            if ys.iter().any(|y| *y <= 0.0) {
                return None;
            }
            let ly: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
            let c = polynomial(xs, &ly, 1)?;
            vec![c[0].exp(), c[1]]
        }
        TrendKind::MovingAverage(_) => Vec::new(),
    };
    let mut out = Fit {
        kind,
        coefficients,
        r_squared: None,
    };
    if !matches!(kind, TrendKind::MovingAverage(_)) {
        // R² of the power and exponential fits is that of their
        // logarithms, as spreadsheets give it.
        out.r_squared = Some(match kind {
            TrendKind::Power => {
                let lx: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
                let ly: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
                r_squared(&lx, &ly, |x| {
                    out.coefficients[0].ln() + out.coefficients[1] * x
                })
            }
            TrendKind::Exponential => {
                let ly: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
                r_squared(xs, &ly, |x| {
                    out.coefficients[0].ln() + out.coefficients[1] * x
                })
            }
            _ => r_squared(xs, ys, |x| out.at(x)),
        });
    }
    Some(out)
}

/// The moving average of a period: one value from the point `period`
/// on, as (x, y).
pub fn moving_average(xs: &[f64], ys: &[f64], period: u8) -> Vec<(f64, f64)> {
    let p = usize::from(period.max(2));
    if ys.len() < p {
        return Vec::new();
    }
    (p - 1..ys.len())
        .map(|i| (xs[i], ys[i + 1 - p..=i].iter().sum::<f64>() / p as f64))
        .collect()
}

/// How far an error bar reaches above and below each value (GRF-13).
pub fn error_lengths(amount: &ErrorAmount, values: &[f64]) -> Vec<(f64, f64)> {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n.max(1.0);
    let sd = if values.len() > 1 {
        (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
    } else {
        0.0
    };
    values
        .iter()
        .enumerate()
        .map(|(i, v)| match amount {
            ErrorAmount::Fixed(f) => (f.0, f.0),
            ErrorAmount::Percent(p) => {
                let d = (v * p.0 / 100.0).abs();
                (d, d)
            }
            ErrorAmount::StandardDeviation(k) => (sd * k.0, sd * k.0),
            ErrorAmount::StandardError => {
                let se = sd / n.sqrt();
                (se, se)
            }
            ErrorAmount::Custom(list) => list.get(i).map_or((0.0, 0.0), |(p, m)| (p.0, m.0)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_table::Num;

    #[test]
    fn grf_14_trendlines_fit() {
        let xs = [1.0, 2.0, 3.0, 4.0];
        let ys = [3.0, 5.0, 7.0, 9.0];
        let f = fit(TrendKind::Linear, &xs, &ys).unwrap();
        assert!((f.coefficients[1] - 2.0).abs() < 1e-9 && (f.coefficients[0] - 1.0).abs() < 1e-9);
        assert!((f.r_squared.unwrap() - 1.0).abs() < 1e-12);
        assert_eq!(f.equation(','), "y = 2x + 1");
        let sq: Vec<f64> = xs.iter().map(|x| x * x).collect();
        let p = fit(TrendKind::Polynomial(2), &xs, &sq).unwrap();
        assert!((p.at(5.0) - 25.0).abs() < 1e-6);
        assert_eq!(p.equation('.'), "y = 1x² + 0x + 0");
        let ex: Vec<f64> = xs.iter().map(|x| 2.0 * (0.5 * x).exp()).collect();
        let e = fit(TrendKind::Exponential, &xs, &ex).unwrap();
        assert!((e.coefficients[0] - 2.0).abs() < 1e-9 && (e.coefficients[1] - 0.5).abs() < 1e-9);
        let pw: Vec<f64> = xs.iter().map(|x| 3.0 * x.powf(1.5)).collect();
        let w = fit(TrendKind::Power, &xs, &pw).unwrap();
        assert!((w.at(9.0) - 81.0).abs() < 1e-6);
        assert!(fit(TrendKind::Logarithmic, &[0.0, 1.0], &[1.0, 2.0]).is_none());
        assert_eq!(
            moving_average(&xs, &ys, 2),
            [(2.0, 4.0), (3.0, 6.0), (4.0, 8.0)]
        );
    }

    #[test]
    fn grf_13_error_bars() {
        let v = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        assert_eq!(
            error_lengths(&ErrorAmount::Percent(Num(50.0)), &[4.0])[0],
            (2.0, 2.0)
        );
        let sd = error_lengths(&ErrorAmount::StandardDeviation(Num(1.0)), &v)[0].0;
        assert!((sd - 2.138_089_935).abs() < 1e-6);
        let se = error_lengths(&ErrorAmount::StandardError, &v)[0].0;
        assert!((se - sd / 8f64.sqrt()).abs() < 1e-12);
    }
}
