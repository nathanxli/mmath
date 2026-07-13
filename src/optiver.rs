use rand::Rng;

use crate::numfmt::{build_options, fmt_frac, fmt_scaled, gcd};

/// Questions per round, matching the real Optiver numerical test: 80 questions
/// in 8 minutes, multiple choice, +1 for a correct answer and -1 for a wrong
/// one.
pub const QUESTION_LIMIT: usize = 80;
pub const DEFAULT_SECONDS: u64 = 480;

/// A generated Optiver-style question. Answers can be decimals or fractions,
/// so the answer and options are formatted strings.
pub struct OptiverQuestion {
    pub prompt: String,
    pub answer_text: String,
    pub options: [String; 4],
}

/// Generate one question mixing the categories of the real test: arithmetic
/// on integers, decimals, and fractions, weighted toward integer arithmetic.
pub fn generate<R: Rng>(rng: &mut R) -> OptiverQuestion {
    let (prompt, answer_text, candidates) = match rng.random_range(0..100) {
        0..=17 => int_add(rng),
        18..=34 => int_sub(rng),
        35..=49 => int_mul(rng),
        50..=60 => int_div(rng),
        61..=72 => dec_add_sub(rng),
        73..=80 => dec_mul(rng),
        81..=86 => dec_div(rng),
        87..=93 => frac_add_sub(rng),
        _ => frac_of(rng),
    };
    let options = build_options(rng, &answer_text, &candidates);
    OptiverQuestion {
        prompt,
        answer_text,
        options,
    }
}

fn int_strings(values: &[i64]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}

/// 47 + 86 = ?
fn int_add<R: Rng>(rng: &mut R) -> (String, String, Vec<String>) {
    let a = rng.random_range(14..=189) as i64;
    let b = rng.random_range(14..=189) as i64;
    let ans = a + b;
    let candidates = int_strings(&[ans - 1, ans + 1, ans - 2, ans + 2, ans - 10, ans + 10]);
    (format!("{} + {} = ?", a, b), ans.to_string(), candidates)
}

/// 132 - 57 = ?
fn int_sub<R: Rng>(rng: &mut R) -> (String, String, Vec<String>) {
    let a = rng.random_range(14..=189) as i64;
    let b = rng.random_range(14..=189) as i64;
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    let ans = hi - lo;
    let candidates = int_strings(&[ans - 1, ans + 1, ans - 2, ans + 2, ans - 10, ans + 10]);
    (format!("{} - {} = ?", hi, lo), ans.to_string(), candidates)
}

/// 17 * 14 = ? or 68 * 7 = ?
fn int_mul<R: Rng>(rng: &mut R) -> (String, String, Vec<String>) {
    let (a, b) = if rng.random_bool(0.5) {
        (rng.random_range(12..=25) as i64, rng.random_range(11..=19) as i64)
    } else {
        (rng.random_range(13..=99) as i64, rng.random_range(3..=9) as i64)
    };
    let ans = a * b;
    let candidates = int_strings(&[
        (a - 1) * b,
        (a + 1) * b,
        a * (b - 1),
        a * (b + 1),
        ans - 10,
        ans + 10,
    ]);
    let (left, right) = if rng.random_bool(0.5) { (a, b) } else { (b, a) };
    (
        format!("{} * {} = ?", left, right),
        ans.to_string(),
        candidates,
    )
}

/// 144 / 12 = ?
fn int_div<R: Rng>(rng: &mut R) -> (String, String, Vec<String>) {
    let b = rng.random_range(3..=12) as i64;
    let ans = rng.random_range(6..=25) as i64;
    let a = b * ans;
    let candidates = int_strings(&[ans - 1, ans + 1, ans - 2, ans + 2, ans + 10]);
    (format!("{} / {} = ?", a, b), ans.to_string(), candidates)
}

/// 4.7 + 3.8 = ? or 12.4 - 5.6 = ?  (one decimal place)
fn dec_add_sub<R: Rng>(rng: &mut R) -> (String, String, Vec<String>) {
    // Values are tenths: 1.2 to 19.9.
    let a = rng.random_range(12..=199) as i64;
    let b = rng.random_range(12..=199) as i64;
    if rng.random_bool(0.5) {
        let ans = a + b;
        let candidates = vec![
            fmt_scaled(ans - 1, 1),
            fmt_scaled(ans + 1, 1),
            fmt_scaled(ans - 10, 1),
            fmt_scaled(ans + 10, 1),
            fmt_scaled(ans * 10, 1),
        ];
        (
            format!("{} + {} = ?", fmt_scaled(a, 1), fmt_scaled(b, 1)),
            fmt_scaled(ans, 1),
            candidates,
        )
    } else {
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        let ans = hi - lo;
        let candidates = vec![
            fmt_scaled(ans - 1, 1),
            fmt_scaled(ans + 1, 1),
            fmt_scaled(ans - 10, 1),
            fmt_scaled(ans + 10, 1),
            fmt_scaled(ans * 10, 1),
        ];
        (
            format!("{} - {} = ?", fmt_scaled(hi, 1), fmt_scaled(lo, 1)),
            fmt_scaled(ans, 1),
            candidates,
        )
    }
}

/// 0.4 * 0.7 = ? or 2.3 * 6 = ?
fn dec_mul<R: Rng>(rng: &mut R) -> (String, String, Vec<String>) {
    if rng.random_bool(0.5) {
        // 0.a * 0.b, answer in hundredths. The trap is misplacing the point.
        let a = rng.random_range(2..=9) as i64;
        let b = rng.random_range(2..=9) as i64;
        let ans = a * b;
        let candidates = vec![
            fmt_scaled(ans * 10, 2),
            fmt_scaled(ans * 100, 2),
            fmt_scaled((a + 1) * b, 2),
            fmt_scaled((a - 1) * b, 2),
            fmt_scaled(a * (b + 1), 2),
        ];
        (
            format!("0.{} * 0.{} = ?", a, b),
            fmt_scaled(ans, 2),
            candidates,
        )
    } else {
        // x.y * c, answer in tenths.
        let x = rng.random_range(11..=99) as i64;
        let c = rng.random_range(2..=9) as i64;
        let ans = x * c;
        let candidates = vec![
            fmt_scaled(x * (c + 1), 1),
            fmt_scaled(x * (c - 1), 1),
            fmt_scaled(ans - 10, 1),
            fmt_scaled(ans + 10, 1),
            fmt_scaled(ans * 10, 1),
        ];
        (
            format!("{} * {} = ?", fmt_scaled(x, 1), c),
            fmt_scaled(ans, 1),
            candidates,
        )
    }
}

/// 4.8 / 0.6 = ?
fn dec_div<R: Rng>(rng: &mut R) -> (String, String, Vec<String>) {
    let d = rng.random_range(2..=9) as i64;
    let ans = rng.random_range(3..=12) as i64;
    let dividend = d * ans;
    let candidates = vec![
        (ans * 10).to_string(),
        fmt_scaled(ans, 1),
        (ans - 1).to_string(),
        (ans + 1).to_string(),
        (ans + 10).to_string(),
    ];
    (
        format!("{} / 0.{} = ?", fmt_scaled(dividend, 1), d),
        ans.to_string(),
        candidates,
    )
}

const FRAC_DEN_PAIRS: [(i64, i64); 12] = [
    (2, 4),
    (3, 6),
    (2, 6),
    (4, 8),
    (2, 8),
    (3, 4),
    (2, 3),
    (4, 6),
    (6, 8),
    (3, 12),
    (4, 12),
    (2, 5),
];

/// 3/4 + 1/8 = ? or 5/6 - 1/3 = ?
fn frac_add_sub<R: Rng>(rng: &mut R) -> (String, String, Vec<String>) {
    let (d1, d2) = FRAC_DEN_PAIRS[rng.random_range(0..FRAC_DEN_PAIRS.len())];
    let n1 = rng.random_range(1..d1);
    let n2 = rng.random_range(1..d2);
    let lcm = d1 * d2 / gcd(d1, d2);
    let (p1, p2) = (n1 * lcm / d1, n2 * lcm / d2);
    let subtract = rng.random_bool(0.5) && p1 != p2;
    let (prompt, num) = if subtract {
        // Keep the result positive.
        if p1 >= p2 {
            (format!("{}/{} - {}/{} = ?", n1, d1, n2, d2), p1 - p2)
        } else {
            (format!("{}/{} - {}/{} = ?", n2, d2, n1, d1), p2 - p1)
        }
    } else {
        (format!("{}/{} + {}/{} = ?", n1, d1, n2, d2), p1 + p2)
    };
    let answer = fmt_frac(num, lcm);
    let candidates = vec![
        // Classic slip: combine numerators and denominators directly.
        if subtract {
            fmt_frac((n1 - n2).abs().max(1), (d1 - d2).abs().max(1))
        } else {
            fmt_frac(n1 + n2, d1 + d2)
        },
        fmt_frac(num + 1, lcm),
        fmt_frac((num - 1).max(0), lcm),
        // Result of the opposite operation.
        fmt_frac(if subtract { p1 + p2 } else { (p1 - p2).abs() }, lcm),
        fmt_frac(num + 2, lcm),
    ];
    (prompt, answer, candidates)
}

/// 3/4 of 60 = ?
fn frac_of<R: Rng>(rng: &mut R) -> (String, String, Vec<String>) {
    let dens = [2i64, 3, 4, 5, 8, 10];
    let q = dens[rng.random_range(0..dens.len())];
    let p = loop {
        let p = rng.random_range(1..q);
        if gcd(p, q) == 1 {
            break p;
        }
    };
    let unit = rng.random_range(4..=20) as i64;
    let n = q * unit;
    let ans = unit * p;
    let candidates = int_strings(&[
        unit * (p + 1),
        unit * (p - 1),
        ans + unit,
        ans - unit,
        ans - 1,
        ans + 1,
    ]);
    (
        format!("{}/{} of {} = ?", p, q, n),
        ans.to_string(),
        candidates,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_questions_are_consistent() {
        let mut rng = rand::rng();
        for _ in 0..20_000 {
            let q = generate(&mut rng);
            assert!(q.prompt.ends_with("= ?"), "bad prompt: {:?}", q.prompt);
            assert!(
                q.options.contains(&q.answer_text),
                "answer {:?} missing from options {:?} for {:?}",
                q.answer_text,
                q.options,
                q.prompt
            );
            for (i, v) in q.options.iter().enumerate() {
                assert!(!v.is_empty());
                assert!(
                    !v.starts_with('-'),
                    "negative option {:?} for {:?}",
                    q.options,
                    q.prompt
                );
                assert!(
                    !q.options[i + 1..].contains(v),
                    "duplicate options {:?} for {:?}",
                    q.options,
                    q.prompt
                );
            }
        }
    }
}
