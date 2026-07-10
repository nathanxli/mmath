use rand::Rng;

/// A generated sequence question: the shown terms (as a prompt ending in "?"),
/// the next term, and distractor candidates for multiple-choice mode.
pub struct Sequence {
    pub prompt: String,
    pub answer: i32,
    pub candidates: Vec<i32>,
}

fn prompt_of(terms: &[i32]) -> String {
    let mut parts: Vec<String> = terms.iter().map(|t| t.to_string()).collect();
    parts.push(String::from("?"));
    parts.join(", ")
}

/// Draw a nonzero value in [lo, hi] by magnitude, negated half the time.
fn signed_nonzero<R: Rng>(rng: &mut R, lo: i32, hi: i32) -> i32 {
    let magnitude = rng.random_range(lo..=hi);
    if rng.random_bool(0.5) { -magnitude } else { magnitude }
}

/// Generate one sequence question in the style of quant trading OAs: the
/// pattern families below (constant/growing differences, ratios, recurrences,
/// interleaving, polynomial values, primes) cover the bulk of what tests like
/// those from Optiver, IMC, and Flow ask for a "next number" item.
pub fn generate<R: Rng>(rng: &mut R) -> Sequence {
    match rng.random_range(0..10) {
        0 => arithmetic(rng),
        1 => geometric(rng),
        2 => quadratic(rng),
        3 => fibonacci_like(rng),
        4 => interleaved(rng),
        5 => squares(rng),
        6 => cubes(rng),
        7 => mul_add(rng),
        8 => primes(rng),
        _ => alternating_add(rng),
    }
}

/// Constant difference: 7, 12, 17, 22, ?
fn arithmetic<R: Rng>(rng: &mut R) -> Sequence {
    let d = signed_nonzero(rng, 2, 12);
    let start = rng.random_range(-20..=50);
    let terms: Vec<i32> = (0..4).map(|i| start + i * d).collect();
    let answer = start + 4 * d;
    let candidates = vec![
        answer - 1,
        answer + 1,
        answer - 2,
        answer + 2,
        answer + d,
        answer - 2 * d,
    ];
    Sequence {
        prompt: prompt_of(&terms),
        answer,
        candidates,
    }
}

/// Constant ratio: 3, 6, 12, 24, ? (ratio may be negative: 2, -6, 18, -54, ?)
fn geometric<R: Rng>(rng: &mut R) -> Sequence {
    let ratio = *[2, 3, -2].get(rng.random_range(0..3)).unwrap();
    let start = if ratio == 2 {
        rng.random_range(1..=12)
    } else {
        rng.random_range(1..=5)
    };
    let mut terms = vec![start];
    for _ in 0..3 {
        terms.push(terms.last().unwrap() * ratio);
    }
    let last = *terms.last().unwrap();
    let answer = last * ratio;
    let candidates = vec![
        answer - ratio,
        answer + ratio,
        last * (ratio + 1),
        last * (ratio - 1),
        -answer,
        answer + last,
    ];
    Sequence {
        prompt: prompt_of(&terms),
        answer,
        candidates,
    }
}

/// Growing difference (constant second difference): 2, 5, 10, 17, ?
fn quadratic<R: Rng>(rng: &mut R) -> Sequence {
    let d0 = rng.random_range(-4..=8);
    let k = signed_nonzero(rng, 1, 4);
    let start = rng.random_range(-10..=20);
    let mut terms = vec![start];
    let mut diff = d0;
    for _ in 0..3 {
        terms.push(terms.last().unwrap() + diff);
        diff += k;
    }
    let last = *terms.last().unwrap();
    let answer = last + diff;
    let candidates = vec![
        // The classic trap: repeat the previous difference instead of growing it.
        last + diff - k,
        answer + k,
        answer - k,
        answer - 1,
        answer + 1,
    ];
    Sequence {
        prompt: prompt_of(&terms),
        answer,
        candidates,
    }
}

/// Each term is the sum of the previous two: 4, 7, 11, 18, 29, ?
fn fibonacci_like<R: Rng>(rng: &mut R) -> Sequence {
    let mut terms = vec![rng.random_range(1..=9), rng.random_range(1..=9)];
    for _ in 0..3 {
        let n = terms.len();
        terms.push(terms[n - 1] + terms[n - 2]);
    }
    let n = terms.len();
    let (prev, last) = (terms[n - 2], terms[n - 1]);
    let answer = prev + last;
    let candidates = vec![
        // Arithmetic trap: extend by the last difference instead of summing.
        last + (last - prev),
        2 * last,
        answer - 1,
        answer + 1,
        answer - 2,
        answer + 2,
    ];
    Sequence {
        prompt: prompt_of(&terms),
        answer,
        candidates,
    }
}

/// Two interleaved arithmetic sequences: 3, 20, 8, 17, 13, ? (A: +5, B: -3)
fn interleaved<R: Rng>(rng: &mut R) -> Sequence {
    let a_start = rng.random_range(-10..=30);
    let b_start = rng.random_range(-10..=30);
    let da = signed_nonzero(rng, 2, 9);
    let mut db = signed_nonzero(rng, 2, 9);
    if db == da {
        db = -db;
    }
    // a1, b1, a2, b2, a3 -> next is b3.
    let terms = vec![
        a_start,
        b_start,
        a_start + da,
        b_start + db,
        a_start + 2 * da,
    ];
    let answer = b_start + 2 * db;
    let candidates = vec![
        // Continue the A strand instead of the B strand.
        a_start + 3 * da,
        terms[4] + db,
        answer - db,
        answer + db,
        answer - 1,
        answer + 1,
    ];
    Sequence {
        prompt: prompt_of(&terms),
        answer,
        candidates,
    }
}

/// Consecutive squares, optionally all shifted by a constant: 16, 25, 36, 49, ?
fn squares<R: Rng>(rng: &mut R) -> Sequence {
    let m = rng.random_range(1..=9);
    let shift = rng.random_range(-2..=2);
    let terms: Vec<i32> = (m..m + 4).map(|n| n * n + shift).collect();
    let next = m + 4;
    let answer = next * next + shift;
    let last = terms[3];
    let prev = terms[2];
    let candidates = vec![
        // Repeat the last gap instead of widening it by 2.
        last + (last - prev),
        answer - 2,
        answer + 2,
        answer - 1,
        answer + 1,
    ];
    Sequence {
        prompt: prompt_of(&terms),
        answer,
        candidates,
    }
}

/// Consecutive cubes: 8, 27, 64, 125, ?
fn cubes<R: Rng>(rng: &mut R) -> Sequence {
    let m = rng.random_range(1..=6);
    let terms: Vec<i32> = (m..m + 4).map(|n| n * n * n).collect();
    let next = m + 4;
    let answer = next * next * next;
    let last = terms[3];
    let prev = terms[2];
    let candidates = vec![
        last + (last - prev),
        answer - next,
        answer + next,
        answer - 1,
        answer + 1,
    ];
    Sequence {
        prompt: prompt_of(&terms),
        answer,
        candidates,
    }
}

/// Multiply-then-add recurrence: 3, 7, 15, 31, ? (x2 + 1)
fn mul_add<R: Rng>(rng: &mut R) -> Sequence {
    let ratio = rng.random_range(2..=3);
    let add = if ratio == 2 {
        signed_nonzero(rng, 1, 5)
    } else {
        signed_nonzero(rng, 1, 4)
    };
    let start = rng.random_range(1..=4);
    let mut terms = vec![start];
    for _ in 0..3 {
        terms.push(terms.last().unwrap() * ratio + add);
    }
    let last = *terms.last().unwrap();
    let prev = terms[2];
    let answer = last * ratio + add;
    let candidates = vec![
        // Forget the additive step.
        last * ratio,
        answer - 1,
        answer + 1,
        answer - add,
        answer + add,
        last + (last - prev),
    ];
    Sequence {
        prompt: prompt_of(&terms),
        answer,
        candidates,
    }
}

const PRIMES: [i32; 35] = [
    2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89,
    97, 101, 103, 107, 109, 113, 127, 131, 137, 139, 149,
];

/// Consecutive primes: 11, 13, 17, 19, 23, ?
fn primes<R: Rng>(rng: &mut R) -> Sequence {
    let start = rng.random_range(0..=PRIMES.len() - 6);
    let terms = PRIMES[start..start + 5].to_vec();
    let answer = PRIMES[start + 5];
    let last = terms[4];
    let prev = terms[3];
    let candidates = vec![
        last + (last - prev),
        answer - 2,
        answer + 2,
        answer - 1,
        answer + 1,
        answer + 4,
    ];
    Sequence {
        prompt: prompt_of(&terms),
        answer,
        candidates,
    }
}

/// Two alternating steps: 5, 12, 9, 16, 13, ? (+7, -3 repeating)
fn alternating_add<R: Rng>(rng: &mut R) -> Sequence {
    let a = signed_nonzero(rng, 2, 9);
    let mut b = signed_nonzero(rng, 2, 9);
    if b == a {
        b = -b;
    }
    let start = rng.random_range(0..=30);
    // t1 +a t2 +b t3 +a t4 +b t5 -> next step is +a.
    let mut terms = vec![start];
    for i in 0..4 {
        let step = if i % 2 == 0 { a } else { b };
        terms.push(terms.last().unwrap() + step);
    }
    let last = *terms.last().unwrap();
    let answer = last + a;
    let candidates = vec![
        // Apply the wrong half of the pattern.
        last + b,
        answer - 1,
        answer + 1,
        answer - 2,
        answer + 2,
    ];
    Sequence {
        prompt: prompt_of(&terms),
        answer,
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_sequences_are_consistent() {
        let mut rng = rand::rng();
        for _ in 0..10_000 {
            let seq = generate(&mut rng);
            let shown = seq.prompt.matches(", ").count();
            assert!(
                (4..=5).contains(&shown),
                "unexpected term count in {:?}",
                seq.prompt
            );
            assert!(seq.prompt.ends_with(", ?"), "bad prompt: {:?}", seq.prompt);
            assert!(
                seq.answer.abs() <= 5_000,
                "answer out of mental range: {} in {:?}",
                seq.answer,
                seq.prompt
            );
            assert!(
                seq.candidates.len() >= 3,
                "too few distractor candidates for {:?}",
                seq.prompt
            );
        }
    }
}
