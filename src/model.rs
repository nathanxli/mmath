use std::time::{Duration, Instant};

use rand::Rng;
use rand::seq::SliceRandom;

pub const ADD_MIN: i32 = 2;
pub const MUL_MIN: i32 = 2;

const SKEW_GAMMA: f64 = 1.3;

/// Draw an integer in [min, max] with a mild symmetric bias toward the extremes
/// (U-shaped). SKEW_GAMMA = 1.0 is uniform; > 1.0 lifts the tails. Used only for
/// addition/subtraction operands, whose sum is otherwise triangular so extreme
/// results are rare.
fn skewed_range<R: Rng>(rng: &mut R, min: i32, max: i32) -> i32 {
    if min >= max {
        return min;
    }
    let u: f64 = rng.random();
    let v = if u < 0.5 {
        0.5 * (2.0 * u).powf(SKEW_GAMMA)
    } else {
        1.0 - 0.5 * (2.0 * (1.0 - u)).powf(SKEW_GAMMA)
    };
    min + (v * (max - min) as f64).round() as i32
}

#[derive(Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

pub struct Question {
    pub prompt: String,
    answer: i32,
    /// The 2x2 answer grid (one entry equals `answer`), present in
    /// multiple-choice mode only.
    pub options: Option<[i32; 4]>,
}

pub struct QuestionRecord {
    pub prompt: String,
    pub elapsed: Duration,
    pub correct: bool,
    /// End-of-speech-to-answer latency (recognition + input pipeline),
    /// recorded for voice answers.
    pub voice_latency: Option<Duration>,
}

#[derive(Clone)]
pub struct GameConfig {
    pub add_max: i32,
    pub mul_max_left: i32,
    pub mul_max_right: i32,
    pub add_enabled: bool,
    pub sub_enabled: bool,
    pub mul_enabled: bool,
    pub div_enabled: bool,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            add_max: 100,
            mul_max_left: 12,
            mul_max_right: 100,
            add_enabled: true,
            sub_enabled: true,
            mul_enabled: true,
            div_enabled: true,
        }
    }
}

impl GameConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.add_max < ADD_MIN {
            return Err("Addition high end must be at least 2.");
        }
        if self.mul_max_left < MUL_MIN {
            return Err("Left multiplication high end must be at least 2.");
        }
        if self.mul_max_right < MUL_MIN {
            return Err("Right multiplication high end must be at least 2.");
        }
        if !self.add_enabled && !self.sub_enabled && !self.mul_enabled && !self.div_enabled {
            return Err("At least one mode must be enabled.");
        }
        Ok(())
    }
}

/// Build the 4-entry option set for a question: the answer plus 3 distinct
/// distractors drawn from `candidates` (plausible slips for the operation),
/// topped up with small random offsets if the candidates run out. All options
/// stay >= 1 to match the range of generated answers.
fn make_options<R: Rng>(rng: &mut R, answer: i32, candidates: &[i32]) -> [i32; 4] {
    let mut options = vec![answer];
    let mut pool = candidates.to_vec();
    pool.shuffle(rng);
    for value in pool {
        if options.len() == 4 {
            break;
        }
        if value >= 1 && !options.contains(&value) {
            options.push(value);
        }
    }
    while options.len() < 4 {
        let delta = rng.random_range(1..=10);
        let value = if rng.random_bool(0.5) {
            answer + delta
        } else {
            answer - delta
        };
        if value >= 1 && !options.contains(&value) {
            options.push(value);
        }
    }
    options.shuffle(rng);
    options.try_into().unwrap()
}

struct QuestionGenerator {
    rng: rand::rngs::ThreadRng,
    config: GameConfig,
    mult_choice: bool,
}

impl QuestionGenerator {
    fn new(config: GameConfig, mult_choice: bool) -> Self {
        Self {
            rng: rand::rng(),
            config,
            mult_choice,
        }
    }

    fn next(&mut self) -> Question {
        let mut enabled_ops = Vec::with_capacity(4);
        if self.config.add_enabled {
            enabled_ops.push(Op::Add);
        }
        if self.config.sub_enabled {
            enabled_ops.push(Op::Sub);
        }
        if self.config.mul_enabled {
            enabled_ops.push(Op::Mul);
        }
        if self.config.div_enabled {
            enabled_ops.push(Op::Div);
        }
        let op = enabled_ops[self.rng.random_range(0..enabled_ops.len())];

        // Distractor candidates mimic plausible slips for the operation.
        let (prompt, answer, candidates) = match op {
            Op::Add => {
                let a = skewed_range(&mut self.rng, ADD_MIN, self.config.add_max);
                let b = skewed_range(&mut self.rng, ADD_MIN, self.config.add_max);
                let ans = a + b;
                let candidates = vec![
                    ans - 1,
                    ans + 1,
                    ans - 2,
                    ans + 2,
                    ans - 10,
                    ans + 10,
                    ans + (a % 10) - (b % 10),
                ];
                (format!("{} + {} = ?", a, b), ans, candidates)
            }
            Op::Sub => {
                let a = skewed_range(&mut self.rng, ADD_MIN, self.config.add_max);
                let b = skewed_range(&mut self.rng, ADD_MIN, self.config.add_max);
                let sum = a + b;
                let (sub, ans) = if self.rng.random_bool(0.5) {
                    (a, b)
                } else {
                    (b, a)
                };
                let candidates = vec![ans - 1, ans + 1, ans - 2, ans + 2, ans - 10, ans + 10];
                (format!("{} - {} = ?", sum, sub), ans, candidates)
            }
            Op::Mul => {
                let a = self.rng.random_range(MUL_MIN..=self.config.mul_max_left);
                let b = self.rng.random_range(MUL_MIN..=self.config.mul_max_right);
                let (left, right) = if self.rng.random_bool(0.5) {
                    (a, b)
                } else {
                    (b, a)
                };
                let ans = a * b;
                // Off-by-one-operand errors keep a plausible last digit.
                let candidates = vec![
                    (a - 1) * b,
                    (a + 1) * b,
                    a * (b - 1),
                    a * (b + 1),
                    ans - 10,
                    ans + 10,
                ];
                (format!("{} * {} = ?", left, right), ans, candidates)
            }
            Op::Div => {
                let a = self.rng.random_range(MUL_MIN..=self.config.mul_max_left);
                let b = self.rng.random_range(MUL_MIN..=self.config.mul_max_right);
                let product = a * b;
                let (div, ans) = if self.rng.random_bool(0.5) {
                    (a, b)
                } else {
                    (b, a)
                };
                let candidates = vec![ans - 1, ans + 1, ans - 2, ans + 2, ans - 3, ans + 3];
                (format!("{} / {} = ?", product, div), ans, candidates)
            }
        };

        let options = self
            .mult_choice
            .then(|| make_options(&mut self.rng, answer, &candidates));
        Question {
            prompt,
            answer,
            options,
        }
    }
}

pub struct App {
    pub config: GameConfig,
    generator: QuestionGenerator,
    pub current: Question,
    question_started_at: Instant,
    pub history: Vec<QuestionRecord>,
    pub input: String,
    pub score: i32,
    solved: usize,
    pub duration: Duration,
    started_at: Instant,
    pub mult_choice: bool,
    pub wrong_penalty: i32,
}

impl App {
    pub fn new(
        config: GameConfig,
        duration: Duration,
        mult_choice: bool,
        wrong_penalty: i32,
    ) -> Self {
        let mut generator = QuestionGenerator::new(config.clone(), mult_choice);
        let current = generator.next();

        Self {
            config,
            generator,
            current,
            question_started_at: Instant::now(),
            history: Vec::new(),
            input: String::new(),
            score: 0,
            solved: 0,
            duration,
            started_at: Instant::now(),
            mult_choice,
            wrong_penalty,
        }
    }

    pub fn remaining(&self) -> Duration {
        let elapsed = self.started_at.elapsed();
        self.duration.saturating_sub(elapsed)
    }

    pub fn is_done(&self) -> bool {
        self.remaining().is_zero()
    }

    pub fn try_advance_if_correct(&mut self) {
        if let Ok(value) = self.input.trim().parse::<i32>() {
            if value == self.current.answer {
                let elapsed = self.question_started_at.elapsed();
                self.history.push(QuestionRecord {
                    prompt: self.current.prompt.clone(),
                    elapsed,
                    correct: true,
                    voice_latency: None,
                });
                self.score += 1;
                self.solved += 1;
                self.current = self.generator.next();
                self.question_started_at = Instant::now();
                self.input.clear();
            }
        }
    }

    /// Answer the current question with the option at `idx` (multiple-choice
    /// mode). Records the outcome and advances regardless of correctness.
    pub fn answer_with_option(&mut self, idx: usize) {
        let Some(options) = self.current.options else {
            return;
        };
        let correct = options[idx] == self.current.answer;
        self.history.push(QuestionRecord {
            prompt: self.current.prompt.clone(),
            elapsed: self.question_started_at.elapsed(),
            correct,
            voice_latency: None,
        });
        if correct {
            self.score += 1;
            self.solved += 1;
        } else {
            self.score += self.wrong_penalty;
        }
        self.current = self.generator.next();
        self.question_started_at = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skewed_range_stays_in_bounds() {
        let mut rng = rand::rng();
        for _ in 0..100_000 {
            let v = skewed_range(&mut rng, ADD_MIN, 100);
            assert!((ADD_MIN..=100).contains(&v), "out of bounds: {}", v);
        }
    }

    #[test]
    fn skewed_range_boosts_extremes() {
        let mut rng = rand::rng();
        let (min, max) = (ADD_MIN, 100);
        let span = (max - min) as f64;
        let n = 200_000;
        let mut extreme = 0;
        for _ in 0..n {
            let v = skewed_range(&mut rng, min, max);
            let frac = (v - min) as f64 / span;
            if frac <= 0.1 || frac >= 0.9 {
                extreme += 1;
            }
        }
        // A uniform draw lands in the outer 20% of the range ~20% of the time.
        // With SKEW_GAMMA > 1 the tails are lifted, so expect clearly more.
        let ratio = extreme as f64 / n as f64;
        assert!(ratio > 0.24, "expected boosted tails, got {}", ratio);
    }

    #[test]
    fn make_options_distinct_positive_and_contains_answer() {
        let mut rng = rand::rng();
        for _ in 0..10_000 {
            let answer = rng.random_range(2..=1200);
            let candidates = [answer - 1, answer + 1, answer - 2, answer + 2];
            let options = make_options(&mut rng, answer, &candidates);
            assert!(options.contains(&answer), "answer missing: {:?}", options);
            for (i, v) in options.iter().enumerate() {
                assert!(*v >= 1, "non-positive option: {:?}", options);
                assert!(
                    !options[i + 1..].contains(v),
                    "duplicate option: {:?}",
                    options
                );
            }
        }
        // Smallest possible answer: positivity filter rejects most candidates,
        // forcing the fallback fill to complete the set.
        let options = make_options(&mut rng, 2, &[1, 0, -1]);
        assert!(options.contains(&2));
        for (i, v) in options.iter().enumerate() {
            assert!(*v >= 1);
            assert!(!options[i + 1..].contains(v));
        }
    }
}
