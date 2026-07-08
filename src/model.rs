use std::time::{Duration, Instant};

use rand::Rng;

pub const ADD_MIN: i32 = 2;
pub const MUL_MIN: i32 = 2;

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
}

pub struct QuestionRecord {
    pub prompt: String,
    pub elapsed: Duration,
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

struct QuestionGenerator {
    rng: rand::rngs::ThreadRng,
    config: GameConfig,
}

impl QuestionGenerator {
    fn new(config: GameConfig) -> Self {
        Self {
            rng: rand::rng(),
            config,
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

        match op {
            Op::Add => {
                let a = self.rng.random_range(ADD_MIN..=self.config.add_max);
                let b = self.rng.random_range(ADD_MIN..=self.config.add_max);
                Question {
                    prompt: format!("{} + {} = ?", a, b),
                    answer: a + b,
                }
            }
            Op::Sub => {
                let a = self.rng.random_range(ADD_MIN..=self.config.add_max);
                let b = self.rng.random_range(ADD_MIN..=self.config.add_max);
                let sum = a + b;
                if self.rng.random_bool(0.5) {
                    Question {
                        prompt: format!("{} - {} = ?", sum, a),
                        answer: b,
                    }
                } else {
                    Question {
                        prompt: format!("{} - {} = ?", sum, b),
                        answer: a,
                    }
                }
            }
            Op::Mul => {
                let a = self.rng.random_range(MUL_MIN..=self.config.mul_max_left);
                let b = self.rng.random_range(MUL_MIN..=self.config.mul_max_right);
                let (left, right) = if self.rng.random_bool(0.5) {
                    (a, b)
                } else {
                    (b, a)
                };
                Question {
                    prompt: format!("{} * {} = ?", left, right),
                    answer: a * b,
                }
            }
            Op::Div => {
                let a = self.rng.random_range(MUL_MIN..=self.config.mul_max_left);
                let b = self.rng.random_range(MUL_MIN..=self.config.mul_max_right);
                let product = a * b;
                if self.rng.random_bool(0.5) {
                    Question {
                        prompt: format!("{} / {} = ?", product, a),
                        answer: b,
                    }
                } else {
                    Question {
                        prompt: format!("{} / {} = ?", product, b),
                        answer: a,
                    }
                }
            }
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
    pub score: usize,
    solved: usize,
    pub duration: Duration,
    started_at: Instant,
}

impl App {
    pub fn new(config: GameConfig, duration: Duration) -> Self {
        let mut generator = QuestionGenerator::new(config.clone());
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
                });
                self.score += 1;
                self.solved += 1;
                self.current = self.generator.next();
                self.question_started_at = Instant::now();
                self.input.clear();
            }
        }
    }
}
