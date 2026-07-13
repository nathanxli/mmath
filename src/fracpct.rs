use rand::Rng;

use crate::numfmt::{fmt_frac, fmt_scaled, gcd};

/// A generated fraction <-> percentage conversion question. `answer_num /
/// answer_den` is the exact value a typed answer must equal: the percentage
/// number (35 for "35%") or the fraction itself, depending on direction.
pub struct FracPctQuestion {
    pub prompt: String,
    pub answer_text: String,
    pub answer_num: i64,
    pub answer_den: i64,
    pub candidates: Vec<String>,
}

/// The curated conversion table, one family of reduced fractions per
/// denominator -- the canonical set traders memorize. Weights skew sampling
/// toward the families that are harder to recall. Thirds, sixths, and twelfths
/// have repeating percentages: they are asked only fraction -> percent, with
/// the answer rounded to 2 decimal places and "~" marking the approximation.
const FAMILIES: [(i64, u32, bool); 12] = [
    (2, 1, false),
    (3, 3, true),
    (4, 2, false),
    (5, 2, false),
    (6, 4, true),
    (8, 5, false),
    (10, 2, false),
    (12, 5, true),
    (16, 6, false),
    (20, 4, false),
    (25, 4, false),
    (50, 3, false),
];

/// Generate one conversion question: a reduced fraction from a weighted
/// family, asked fraction -> percent or percent -> fraction.
pub fn generate<R: Rng>(rng: &mut R) -> FracPctQuestion {
    let (den, repeating) = pick_family(rng);
    let num = loop {
        let n = rng.random_range(1..den);
        if gcd(n, den) == 1 {
            break n;
        }
    };
    let pct = pct_hundredths(num, den);
    if repeating || rng.random_bool(0.5) {
        to_percent(num, den, pct, repeating)
    } else {
        to_fraction(num, den, pct)
    }
}

fn pick_family<R: Rng>(rng: &mut R) -> (i64, bool) {
    let total: u32 = FAMILIES.iter().map(|&(_, weight, _)| weight).sum();
    let mut roll = rng.random_range(0..total);
    for &(den, weight, repeating) in &FAMILIES {
        if roll < weight {
            return (den, repeating);
        }
        roll -= weight;
    }
    unreachable!()
}

/// The percentage as hundredths of a percent. Every non-repeating family's
/// denominator divides 10000, so those values are exact; repeating families
/// round to the nearest hundredth (1/6 -> 1667, i.e. 16.67%).
fn pct_hundredths(num: i64, den: i64) -> i64 {
    (10_000 * num + den / 2) / den
}

/// "7/20 = ?%", or "5/12 ~ ?%" for repeating families.
fn to_percent(num: i64, den: i64, pct: i64, repeating: bool) -> FracPctQuestion {
    // Distractors: the complement, off-by-one numerators, nearby round
    // percentages, and misplaced halving/doubling.
    let mut raw = vec![
        10_000 - pct,
        pct_hundredths(num + 1, den),
        pct + 500,
        pct - 500,
        pct + 1_000,
        pct - 1_000,
        pct * 2,
        pct / 2,
    ];
    if num > 1 {
        raw.push(pct_hundredths(num - 1, den));
    }
    let candidates = raw
        .into_iter()
        .filter(|&v| v > 0 && v <= 10_000)
        .map(|v| fmt_scaled(v, 2))
        .collect();
    FracPctQuestion {
        prompt: format!("{}/{} {} ?%", num, den, if repeating { "~" } else { "=" }),
        answer_text: fmt_scaled(pct, 2),
        answer_num: pct,
        answer_den: 100,
        candidates,
    }
}

/// "35% = ?", answered as a fraction in lowest terms.
fn to_fraction(num: i64, den: i64, pct: i64) -> FracPctQuestion {
    let mut candidates: Vec<String> = Vec::new();
    // Fractions of nearby round percentages and of the complement, kept only
    // when they reduce to a plausible table-sized denominator.
    for v in [
        10_000 - pct,
        pct + 500,
        pct - 500,
        pct + 1_000,
        pct - 1_000,
        pct / 2,
        pct * 2,
    ] {
        if v > 0 && v < 10_000 && 10_000 / gcd(v, 10_000) <= 80 {
            candidates.push(fmt_frac(v, 10_000));
        }
    }
    // Off-by-one numerators and misread denominators.
    for n in [num - 1, num + 1] {
        if n > 0 && n < den {
            candidates.push(fmt_frac(n, den));
        }
    }
    candidates.push(fmt_frac(num, den * 2));
    if den % 2 == 0 && num < den / 2 {
        candidates.push(fmt_frac(num, den / 2));
    }
    FracPctQuestion {
        prompt: format!("{}% = ?", fmt_scaled(pct, 2)),
        answer_text: fmt_frac(num, den),
        answer_num: num,
        answer_den: den,
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a percent string like "12.5" back into hundredths of a percent.
    fn hundredths_of(text: &str) -> i64 {
        let (int, frac) = text.split_once('.').unwrap_or((text, ""));
        let scale = 10i64.pow(frac.len() as u32);
        let frac_v = if frac.is_empty() {
            0
        } else {
            frac.parse::<i64>().unwrap()
        };
        (int.parse::<i64>().unwrap() * scale + frac_v) * 100 / scale
    }

    #[test]
    fn generated_questions_are_consistent() {
        let mut rng = rand::rng();
        for _ in 0..20_000 {
            let q = generate(&mut rng);
            assert!(
                q.prompt.ends_with("?%") || q.prompt.ends_with("= ?"),
                "bad prompt: {:?}",
                q.prompt
            );
            assert!(q.answer_num > 0 && q.answer_den > 0);
            assert!(
                q.candidates.len() >= 2,
                "too few candidates for {:?}: {:?}",
                q.prompt,
                q.candidates
            );
            for c in &q.candidates {
                assert!(!c.is_empty() && !c.starts_with('-'));
            }
        }
    }

    #[test]
    fn fraction_to_percent_answer_matches_prompt() {
        let mut rng = rand::rng();
        for _ in 0..20_000 {
            let q = generate(&mut rng);
            if !q.prompt.ends_with("?%") {
                continue;
            }
            // Prompt is "num/den = ?%" or "num/den ~ ?%".
            let frac = q.prompt.split_whitespace().next().unwrap();
            let (num, den) = frac.split_once('/').unwrap();
            let (num, den) = (num.parse::<i64>().unwrap(), den.parse::<i64>().unwrap());
            let repeating = q.prompt.contains('~');
            assert_eq!(pct_hundredths(num, den), q.answer_num, "{:?}", q.prompt);
            assert_eq!(q.answer_den, 100);
            assert_eq!(hundredths_of(&q.answer_text), q.answer_num);
            if !repeating {
                // Exact families really are exact.
                assert_eq!(10_000 * num % den, 0, "{:?}", q.prompt);
            }
        }
    }

    #[test]
    fn percent_to_fraction_is_exact_and_reduced() {
        let mut rng = rand::rng();
        let mut seen = false;
        for _ in 0..20_000 {
            let q = generate(&mut rng);
            let Some(pct_text) = q.prompt.strip_suffix("% = ?") else {
                continue;
            };
            seen = true;
            // The shown percentage must equal the answer fraction exactly --
            // repeating families never appear in this direction.
            assert_eq!(
                hundredths_of(pct_text) * q.answer_den,
                q.answer_num * 10_000,
                "{:?} -> {:?}",
                q.prompt,
                q.answer_text
            );
            assert_eq!(gcd(q.answer_num, q.answer_den), 1);
            assert_eq!(
                q.answer_text,
                format!("{}/{}", q.answer_num, q.answer_den)
            );
        }
        assert!(seen, "no percent -> fraction questions generated");
    }
}
