use rand::Rng;
use rand::seq::SliceRandom;

pub fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

/// Format `value / 10^scale` as a decimal string with trailing zeros trimmed.
pub fn fmt_scaled(value: i64, scale: u32) -> String {
    if scale == 0 {
        return value.to_string();
    }
    let denom = 10i64.pow(scale);
    let sign = if value < 0 { "-" } else { "" };
    let magnitude = value.abs();
    let int = magnitude / denom;
    let mut frac = format!("{:0width$}", magnitude % denom, width = scale as usize);
    while frac.ends_with('0') {
        frac.pop();
    }
    if frac.is_empty() {
        format!("{}{}", sign, int)
    } else {
        format!("{}{}.{}", sign, int, frac)
    }
}

/// Format a fraction reduced to lowest terms; whole values render as integers.
pub fn fmt_frac(num: i64, den: i64) -> String {
    if num == 0 {
        return String::from("0");
    }
    let g = gcd(num, den);
    let (num, den) = (num / g, den / g);
    if den == 1 {
        num.to_string()
    } else {
        format!("{}/{}", num, den)
    }
}

/// The answer plus 3 distinct distractors. Draws from `candidates` (plausible
/// slips), skipping negatives and duplicates; if the pool runs dry, falls back
/// to perturbing the answer in its own notation (integer, decimal, fraction).
pub fn build_options<R: Rng>(rng: &mut R, answer: &str, candidates: &[String]) -> [String; 4] {
    let mut options = vec![answer.to_string()];
    let mut pool = candidates.to_vec();
    pool.shuffle(rng);
    for value in pool {
        if options.len() == 4 {
            break;
        }
        if !value.starts_with('-') && !options.contains(&value) {
            options.push(value);
        }
    }
    let mut delta = 1;
    while options.len() < 4 {
        let value = perturb(answer, delta);
        if !value.starts_with('-') && !options.contains(&value) {
            options.push(value);
        }
        delta += 1;
    }
    options.shuffle(rng);
    options.try_into().unwrap()
}

/// Nudge a formatted answer by `delta` in its last place, preserving notation.
fn perturb(answer: &str, delta: i64) -> String {
    if let Some((num, den)) = answer.split_once('/')
        && let (Ok(num), Ok(den)) = (num.parse::<i64>(), den.parse::<i64>())
    {
        return fmt_frac(num + delta, den);
    }
    if let Some((int, frac)) = answer.split_once('.')
        && let Ok(value) = format!("{}{}", int, frac).parse::<i64>()
    {
        return fmt_scaled(value + delta, frac.len() as u32);
    }
    if let Ok(value) = answer.parse::<i64>() {
        return (value + delta).to_string();
    }
    format!("{}{}", answer, delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_scaled_trims_and_pads() {
        assert_eq!(fmt_scaled(28, 2), "0.28");
        assert_eq!(fmt_scaled(280, 2), "2.8");
        assert_eq!(fmt_scaled(2800, 2), "28");
        assert_eq!(fmt_scaled(85, 1), "8.5");
        assert_eq!(fmt_scaled(7, 0), "7");
        assert_eq!(fmt_scaled(-15, 1), "-1.5");
    }

    #[test]
    fn fmt_frac_reduces() {
        assert_eq!(fmt_frac(6, 8), "3/4");
        assert_eq!(fmt_frac(8, 8), "1");
        assert_eq!(fmt_frac(0, 8), "0");
        assert_eq!(fmt_frac(9, 6), "3/2");
    }
}
