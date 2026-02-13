use std::error::Error;
use std::env;
use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use rand::Rng;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph};
use tui_big_text::{BigText, PixelSize};

const ADD_MIN: i32 = 2;
const MUL_MIN: i32 = 2;

#[derive(Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

struct Question {
    prompt: String,
    answer: i32,
}

struct QuestionRecord {
    prompt: String,
    elapsed: Duration,
}

#[derive(Clone)]
struct GameConfig {
    add_max: i32,
    mul_max_left: i32,
    mul_max_right: i32,
    add_enabled: bool,
    sub_enabled: bool,
    mul_enabled: bool,
    div_enabled: bool,
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
    fn validate(&self) -> Result<(), &'static str> {
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
                Question {
                    prompt: format!("{} * {} = ?", a, b),
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

struct App {
    config: GameConfig,
    generator: QuestionGenerator,
    current: Question,
    question_started_at: Instant,
    history: Vec<QuestionRecord>,
    input: String,
    score: usize,
    solved: usize,
    duration: Duration,
    started_at: Instant,
}

impl App {
    fn new(config: GameConfig, duration: Duration) -> Self {
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

    fn remaining(&self) -> Duration {
        let elapsed = self.started_at.elapsed();
        self.duration.saturating_sub(elapsed)
    }

    fn is_done(&self) -> bool {
        self.remaining().is_zero()
    }

    fn try_advance_if_correct(&mut self) {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SetupField {
    AddMode,
    SubMode,
    MulMode,
    DivMode,
    AddHigh,
    MulHighLeft,
    MulHighRight,
    TimeSeconds,
    Start,
}

impl SetupField {
    fn next(self) -> Self {
        match self {
            SetupField::AddMode => SetupField::SubMode,
            SetupField::SubMode => SetupField::MulMode,
            SetupField::MulMode => SetupField::DivMode,
            SetupField::DivMode => SetupField::AddHigh,
            SetupField::AddHigh => SetupField::MulHighLeft,
            SetupField::MulHighLeft => SetupField::MulHighRight,
            SetupField::MulHighRight => SetupField::TimeSeconds,
            SetupField::TimeSeconds => SetupField::Start,
            SetupField::Start => SetupField::AddMode,
        }
    }

    fn prev(self) -> Self {
        match self {
            SetupField::AddMode => SetupField::Start,
            SetupField::SubMode => SetupField::AddMode,
            SetupField::MulMode => SetupField::SubMode,
            SetupField::DivMode => SetupField::MulMode,
            SetupField::AddHigh => SetupField::DivMode,
            SetupField::MulHighLeft => SetupField::AddHigh,
            SetupField::MulHighRight => SetupField::MulHighLeft,
            SetupField::TimeSeconds => SetupField::MulHighRight,
            SetupField::Start => SetupField::TimeSeconds,
        }
    }
}

struct SetupConfig {
    game: GameConfig,
    duration: Duration,
}

struct SetupState {
    focus: SetupField,
    add_enabled: bool,
    sub_enabled: bool,
    mul_enabled: bool,
    div_enabled: bool,
    add_high_input: String,
    add_high_edited: bool,
    mul_high_left_input: String,
    mul_high_left_edited: bool,
    mul_high_right_input: String,
    mul_high_right_edited: bool,
    time_input: String,
    time_edited: bool,
    message: String,
}

impl SetupState {
    fn new() -> Self {
        Self {
            focus: SetupField::AddMode,
            add_enabled: true,
            sub_enabled: true,
            mul_enabled: true,
            div_enabled: true,
            add_high_input: String::from("100"),
            add_high_edited: false,
            mul_high_left_input: String::from("12"),
            mul_high_left_edited: false,
            mul_high_right_input: String::from("100"),
            mul_high_right_edited: false,
            time_input: String::from("120"),
            time_edited: false,
            message: String::from("Set ranges and time, then start."),
        }
    }

    fn active_input_mut(&mut self) -> Option<(&mut String, &mut bool)> {
        match self.focus {
            SetupField::AddMode => None,
            SetupField::SubMode => None,
            SetupField::MulMode => None,
            SetupField::DivMode => None,
            SetupField::AddHigh => Some((&mut self.add_high_input, &mut self.add_high_edited)),
            SetupField::MulHighLeft => {
                Some((&mut self.mul_high_left_input, &mut self.mul_high_left_edited))
            }
            SetupField::MulHighRight => {
                Some((&mut self.mul_high_right_input, &mut self.mul_high_right_edited))
            }
            SetupField::TimeSeconds => Some((&mut self.time_input, &mut self.time_edited)),
            SetupField::Start => None,
        }
    }

    fn toggle_focused_mode(&mut self) -> bool {
        match self.focus {
            SetupField::AddMode => {
                self.add_enabled = !self.add_enabled;
                true
            }
            SetupField::SubMode => {
                self.sub_enabled = !self.sub_enabled;
                true
            }
            SetupField::MulMode => {
                self.mul_enabled = !self.mul_enabled;
                true
            }
            SetupField::DivMode => {
                self.div_enabled = !self.div_enabled;
                true
            }
            _ => false,
        }
    }

    fn parse_config(&self) -> Result<SetupConfig, &'static str> {
        let add_max = self
            .add_high_input
            .parse::<i32>()
            .map_err(|_| "Addition high end must be a whole number.")?;
        let mul_max_left = self
            .mul_high_left_input
            .parse::<i32>()
            .map_err(|_| "Left multiplication high end must be a whole number.")?;
        let mul_max_right = self
            .mul_high_right_input
            .parse::<i32>()
            .map_err(|_| "Right multiplication high end must be a whole number.")?;
        let time_seconds = self
            .time_input
            .parse::<u64>()
            .map_err(|_| "Time must be a whole number of seconds.")?;
        if time_seconds == 0 {
            return Err("Time must be at least 1 second.");
        }

        let config = GameConfig {
            add_max,
            mul_max_left,
            mul_max_right,
            add_enabled: self.add_enabled,
            sub_enabled: self.sub_enabled,
            mul_enabled: self.mul_enabled,
            div_enabled: self.div_enabled,
        };
        config.validate()?;
        Ok(SetupConfig {
            game: config,
            duration: Duration::from_secs(time_seconds),
        })
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let use_small_text = env::args().any(|arg| arg == "-s");
    let mut terminal = init_terminal()?;
    let result = run(&mut terminal, use_small_text);
    restore_terminal(&mut terminal)?;

    match result {
        Ok(Some(_)) => Ok(()),
        Ok(None) => {
            println!("Canceled.");
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    use_small_text: bool,
) -> Result<Option<App>, Box<dyn Error>> {
    let setup = match run_setup(terminal)? {
        Some(config) => config,
        None => return Ok(None),
    };

    let mut game_config = setup.game;
    let mut duration = setup.duration;
    let mut recent_attempts: Vec<RecentAttempt> = Vec::new();

    loop {
        let app = run_game(terminal, game_config.clone(), duration, use_small_text)?;
        recent_attempts.push(RecentAttempt {
            score: app.score,
        });
        if recent_attempts.len() > 10 {
            let overflow = recent_attempts.len() - 10;
            recent_attempts.drain(0..overflow);
        }

        match run_results(terminal, &app, &recent_attempts)? {
            ResultsAction::Restart => {
                game_config = app.config.clone();
                duration = app.duration;
            }
            ResultsAction::Exit => return Ok(Some(app)),
        }
    }
}

enum ResultsAction {
    Restart,
    Exit,
}

struct RecentAttempt {
    score: usize,
}

fn run_results(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &App,
    recent_attempts: &[RecentAttempt],
) -> Result<ResultsAction, Box<dyn Error>> {
    let mut scroll: usize = 0;

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(6),
                    Constraint::Min(6),
                    Constraint::Length(3),
                ])
                .split(area);

            let summary = vec![
                Line::from(format!(
                    "Addition range: {} to {}",
                    ADD_MIN, app.config.add_max
                )),
                Line::from(format!(
                    "Multiplication range: ({} to {}) x ({} to {})",
                    MUL_MIN, app.config.mul_max_left, MUL_MIN, app.config.mul_max_right
                )),
                Line::from(format!("Time: {} seconds", app.duration.as_secs())),
            ];

            let summary_widget = Paragraph::new(summary).block(
                Block::default()
                    .title("Session Settings")
                    .borders(Borders::ALL)
                    .padding(Padding::left(1)),
            );
            frame.render_widget(summary_widget, chunks[0]);

            let middle_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(67), Constraint::Percentage(33)])
                .split(chunks[1]);
            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(6), Constraint::Length(4)])
                .split(middle_chunks[0]);
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(6), Constraint::Length(4)])
                .split(middle_chunks[1]);

            let mut history_lines = Vec::new();
            if app.history.is_empty() {
                history_lines.push(Line::from("No solved questions."));
            } else {
                let count = app.history.len() as f64;
                let mean = app
                    .history
                    .iter()
                    .map(|record| record.elapsed.as_secs_f64())
                    .sum::<f64>()
                    / count;
                let variance = app
                    .history
                    .iter()
                    .map(|record| {
                        let delta = record.elapsed.as_secs_f64() - mean;
                        delta * delta
                    })
                    .sum::<f64>()
                    / count;
                let stdev = variance.sqrt();
                let threshold = mean + (2.0 * stdev);

                for (idx, record) in app.history.iter().enumerate() {
                    let elapsed = record.elapsed.as_secs_f64();
                    let time_style = if elapsed > threshold {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    history_lines.push(Line::from(vec![
                        Span::raw(format!("{:>3}. {:<18}  ", idx + 1, record.prompt)),
                        Span::styled(format_elapsed(record.elapsed), time_style),
                    ]));
                }
            }

            let history_widget = Paragraph::new(history_lines)
                .scroll((scroll as u16, 0))
                .block(
                    Block::default()
                        .title("Questions + Time")
                        .borders(Borders::ALL)
                        .padding(Padding::left(1)),
                );
            frame.render_widget(history_widget, left_chunks[0]);

            let question_stats_lines = if app.history.is_empty() {
                vec![
                    Line::from("μ: n/a"),
                    Line::from("σ: n/a"),
                ]
            } else {
                let count = app.history.len() as f64;
                let mean = app
                    .history
                    .iter()
                    .map(|record| record.elapsed.as_secs_f64())
                    .sum::<f64>()
                    / count;
                let variance = app
                    .history
                    .iter()
                    .map(|record| {
                        let delta = record.elapsed.as_secs_f64() - mean;
                        delta * delta
                    })
                    .sum::<f64>()
                    / count;
                let stdev = variance.sqrt();
                vec![
                    Line::from(format!("μ: {:.2}s", mean)),
                    Line::from(format!("σ: {:.2}s", stdev)),
                ]
            };
            let question_stats_widget = Paragraph::new(question_stats_lines).block(
                Block::default()
                    .title("Time per Question")
                    .borders(Borders::ALL)
                    .padding(Padding::left(1)),
            );
            frame.render_widget(question_stats_widget, left_chunks[1]);

            let mut recent_lines = Vec::new();
            if recent_attempts.is_empty() {
                recent_lines.push(Line::from("No attempts yet."));
            } else {
                recent_lines.push(Line::from("Scores:"));
                let best = recent_attempts
                    .iter()
                    .map(|attempt| attempt.score)
                    .max()
                    .unwrap_or(0);
                let worst = recent_attempts
                    .iter()
                    .map(|attempt| attempt.score)
                    .min()
                    .unwrap_or(0);

                for (idx, attempt) in recent_attempts.iter().rev().enumerate() {
                    let style = if recent_attempts.len() == 1 || attempt.score == best {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else if attempt.score == worst {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else if idx == 0 {
                        Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    recent_lines.push(Line::from(Span::styled(
                        format!("{:>2}. {}", idx + 1, attempt.score),
                        style,
                    )));
                }
            }

            let recent_widget = Paragraph::new(recent_lines).block(
                Block::default()
                    .title("Session Statistics")
                    .borders(Borders::ALL)
                    .padding(Padding::left(1)),
            );
            frame.render_widget(recent_widget, right_chunks[0]);

            let session_stats_lines = if recent_attempts.is_empty() {
                vec![
                    Line::from("μ: n/a"),
                    Line::from("σ: n/a"),
                ]
            } else {
                let count = recent_attempts.len() as f64;
                let mean = recent_attempts
                    .iter()
                    .map(|attempt| attempt.score as f64)
                    .sum::<f64>()
                    / count;
                let variance = recent_attempts
                    .iter()
                    .map(|attempt| {
                        let delta = attempt.score as f64 - mean;
                        delta * delta
                    })
                    .sum::<f64>()
                    / count;
                let stdev = variance.sqrt();
                vec![
                    Line::from(format!("μ: {:.2}", mean)),
                    Line::from(format!("σ: {:.2}", stdev)),
                ]
            };
            let session_stats_widget = Paragraph::new(session_stats_lines).block(
                Block::default()
                    .title("Session Scores")
                    .borders(Borders::ALL)
                    .padding(Padding::left(1)),
            );
            frame.render_widget(session_stats_widget, right_chunks[1]);

            let footer = Paragraph::new("Esc to exit results. 'r' to restart with same parameters. Up/Down to scroll.")
                .block(
                    Block::default()
                        .title("Done")
                        .borders(Borders::ALL)
                        .padding(Padding::left(1)),
                );
            frame.render_widget(footer, chunks[2]);
        })?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        scroll = scroll.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let max_scroll = app.history.len().saturating_sub(1);
                        scroll = (scroll + 1).min(max_scroll);
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => return Ok(ResultsAction::Restart),
                    KeyCode::Esc => return Ok(ResultsAction::Exit),
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

fn format_elapsed(duration: Duration) -> String {
    format!("{:.2}s", duration.as_secs_f64())
}

fn run_setup(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<Option<SetupConfig>, Box<dyn Error>> {
    let mut setup = SetupState::new();

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Min(10), Constraint::Length(3)])
                .split(area);

            let lines = vec![
                Line::from("Press Enter on Start (or 's') to begin. Esc cancels."),
                Line::from("Up/Down (or j/k) to move. Space toggles modes."),
                Line::from(""),
                modes_line(
                    setup.add_enabled,
                    setup.sub_enabled,
                    setup.mul_enabled,
                    setup.div_enabled,
                    setup.focus,
                ),
                field_line(
                    "Addition range",
                    &format!("{} to {}", ADD_MIN, setup.add_high_input),
                    setup.focus == SetupField::AddHigh,
                ),
                multiplication_range_line(
                    setup.mul_high_left_input.as_str(),
                    setup.mul_high_right_input.as_str(),
                    setup.focus == SetupField::MulHighLeft,
                    setup.focus == SetupField::MulHighRight,
                ),
                field_line(
                    "Time (seconds)",
                    setup.time_input.as_str(),
                    setup.focus == SetupField::TimeSeconds,
                ),
                Line::from(""),
                start_line(setup.focus == SetupField::Start),
            ];

            let setup_widget = Paragraph::new(lines).block(
                Block::default()
                    .title("Game Parameters")
                    .borders(Borders::ALL)
                    .padding(Padding::left(1)),
            );
            frame.render_widget(setup_widget, chunks[0]);

            let status = Paragraph::new(setup.message.as_str()).block(
                Block::default()
                    .title("Status")
                    .borders(Borders::ALL)
                    .padding(Padding::left(1))
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
            frame.render_widget(status, chunks[1]);
        })?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => setup.focus = setup.focus.prev(),
                    KeyCode::Down | KeyCode::Char('j') => setup.focus = setup.focus.next(),
                    KeyCode::Backspace => {
                        if let Some((input, edited)) = setup.active_input_mut() {
                            input.pop();
                            *edited = true;
                        }
                    }
                    KeyCode::Char(c) if c.is_ascii_digit() => {
                        if let Some((input, edited)) = setup.active_input_mut() {
                            if !*edited {
                                input.clear();
                                *edited = true;
                            }
                            input.push(c);
                        }
                    }
                    KeyCode::Char(' ') => {
                        setup.toggle_focused_mode();
                    }
                    KeyCode::Enter => {
                        if setup.toggle_focused_mode() {
                        } else if setup.focus == SetupField::Start {
                            match setup.parse_config() {
                                Ok(config) => return Ok(Some(config)),
                                Err(msg) => setup.message = msg.to_string(),
                            }
                        }
                    }
                    KeyCode::Char('s') => match setup.parse_config() {
                        Ok(config) => return Ok(Some(config)),
                        Err(msg) => setup.message = msg.to_string(),
                    },
                    KeyCode::Esc => return Ok(None),
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

fn field_line(label: &str, value: &str, focused: bool) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled(format!("{}: ", label), label_style),
        Span::styled(
            value.to_string(),
            if focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default()
            },
        ),
    ])
}

fn modes_line(
    add_on: bool,
    sub_on: bool,
    mul_on: bool,
    div_on: bool,
    focused: SetupField,
) -> Line<'static> {
    let label_style = if matches!(
        focused,
        SetupField::AddMode | SetupField::SubMode | SetupField::MulMode | SetupField::DivMode
    ) {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled("Modes: ", label_style),
        mode_span("+", add_on, focused == SetupField::AddMode),
        Span::raw("  "),
        mode_span("-", sub_on, focused == SetupField::SubMode),
        Span::raw("  "),
        mode_span("*", mul_on, focused == SetupField::MulMode),
        Span::raw("  "),
        mode_span("/", div_on, focused == SetupField::DivMode),
    ])
}

fn mode_span(symbol: &str, enabled: bool, focused: bool) -> Span<'static> {
    let mut style = if enabled {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    };
    if focused {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    Span::styled(format!("{} [{}]", symbol, if enabled { "on" } else { "off" }), style)
}

fn multiplication_range_line(
    left: &str,
    right: &str,
    left_focused: bool,
    right_focused: bool,
) -> Line<'static> {
    let label_focused = left_focused || right_focused;
    let label_style = if label_focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let focused_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

    Line::from(vec![
        Span::styled("Multiplication range: ", label_style),
        Span::raw(format!("({} to ", MUL_MIN)),
        Span::styled(
            left.to_string(),
            if left_focused {
                focused_style
            } else {
                Style::default()
            },
        ),
        Span::raw(") x ("),
        Span::raw(format!("{} to ", MUL_MIN)),
        Span::styled(
            right.to_string(),
            if right_focused {
                focused_style
            } else {
                Style::default()
            },
        ),
        Span::raw(")"),
    ])
}

fn start_line(focused: bool) -> Line<'static> {
    let style = if focused {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    };
    Line::from(Span::styled("Start", style))
}

fn run_game(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: GameConfig,
    duration: Duration,
    use_small_text: bool,
) -> Result<App, Box<dyn Error>> {
    let mut app = App::new(config, duration);

    while !app.is_done() {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(6),
                    Constraint::Length(3),
                ])
                .split(area);

            let timer = app.remaining().as_secs();
            let header = Paragraph::new(Line::from(vec![
                Span::raw("Time: "),
                Span::styled(
                    format!("{}s", timer),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("    Score: "),
                Span::styled(
                    format!("{}", app.score),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]))
            .block(Block::default().title("Mental Math").borders(Borders::ALL));
            frame.render_widget(header, chunks[0]);

            if use_small_text {
                let question = Paragraph::new(app.current.prompt.clone())
                    .block(
                        Block::default()
                            .title("Question")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Reset))
                            .style(Style::default().fg(Color::Reset)),
                    )
                    .alignment(Alignment::Center);
                frame.render_widget(question, chunks[1]);
            } else {
                let question_block = Block::default()
                    .title("Question")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Reset))
                    .style(Style::default().fg(Color::Reset));
                let question_inner = question_block.inner(chunks[1]);
                frame.render_widget(question_block, chunks[1]);

                let question = BigText::builder()
                    .pixel_size(PixelSize::HalfHeight)
                    .centered()
                    .style(Style::default().add_modifier(Modifier::BOLD))
                    .lines(vec![Line::from(app.current.prompt.clone())])
                    .build();
                frame.render_widget(question, question_inner);
            }

            let input = Paragraph::new(app.input.clone()).block(
                Block::default()
                    .title("Answer (Esc to quit)")
                    .borders(Borders::ALL),
            );
            frame.render_widget(input, chunks[2]);
        })?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char(c) if c.is_ascii_digit() => {
                        app.input.push(c);
                        app.try_advance_if_correct();
                    }
                    KeyCode::Char('-') if app.input.is_empty() => {
                        app.input.push('-');
                        app.try_advance_if_correct();
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
                        app.try_advance_if_correct();
                    }
                    KeyCode::Esc => break,
                    _ => {}
                },
                _ => {}
            }
        }
    }

    Ok(app)
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>, Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
