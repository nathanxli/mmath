use std::error::Error;
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
use ratatui::widgets::{Block, Borders, Paragraph};

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

#[derive(Clone)]
struct GameConfig {
    add_max: i32,
    mul_max: i32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            add_max: 100,
            mul_max: 12,
        }
    }
}

impl GameConfig {
    fn validate(&self) -> Result<(), &'static str> {
        if self.add_max < ADD_MIN {
            return Err("Addition high end must be at least 2.");
        }
        if self.mul_max < MUL_MIN {
            return Err("Multiplication high end must be at least 2.");
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
        let op = match self.rng.random_range(0..4) {
            0 => Op::Add,
            1 => Op::Sub,
            2 => Op::Mul,
            _ => Op::Div,
        };

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
                let a = self.rng.random_range(MUL_MIN..=self.config.mul_max);
                let b = self.rng.random_range(MUL_MIN..=self.config.mul_max);
                Question {
                    prompt: format!("{} * {} = ?", a, b),
                    answer: a * b,
                }
            }
            Op::Div => {
                let a = self.rng.random_range(MUL_MIN..=self.config.mul_max);
                let b = self.rng.random_range(MUL_MIN..=self.config.mul_max);
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
    generator: QuestionGenerator,
    current: Question,
    input: String,
    score: usize,
    solved: usize,
    duration: Duration,
    started_at: Instant,
}

impl App {
    fn new(config: GameConfig, duration: Duration) -> Self {
        let mut generator = QuestionGenerator::new(config);
        let current = generator.next();

        Self {
            generator,
            current,
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
                self.score += 1;
                self.solved += 1;
                self.current = self.generator.next();
                self.input.clear();
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SetupField {
    AddHigh,
    MulHigh,
    Start,
}

impl SetupField {
    fn next(self) -> Self {
        match self {
            SetupField::AddHigh => SetupField::MulHigh,
            SetupField::MulHigh => SetupField::Start,
            SetupField::Start => SetupField::AddHigh,
        }
    }

    fn prev(self) -> Self {
        match self {
            SetupField::AddHigh => SetupField::Start,
            SetupField::MulHigh => SetupField::AddHigh,
            SetupField::Start => SetupField::MulHigh,
        }
    }
}

struct SetupState {
    focus: SetupField,
    add_high_input: String,
    mul_high_input: String,
    message: String,
}

impl SetupState {
    fn new() -> Self {
        Self {
            focus: SetupField::AddHigh,
            add_high_input: String::from("100"),
            mul_high_input: String::from("12"),
            message: String::from("Set the two high ends, then start."),
        }
    }

    fn active_input_mut(&mut self) -> Option<&mut String> {
        match self.focus {
            SetupField::AddHigh => Some(&mut self.add_high_input),
            SetupField::MulHigh => Some(&mut self.mul_high_input),
            SetupField::Start => None,
        }
    }

    fn parse_config(&self) -> Result<GameConfig, &'static str> {
        let add_max = self
            .add_high_input
            .parse::<i32>()
            .map_err(|_| "Addition high end must be a whole number.")?;
        let mul_max = self
            .mul_high_input
            .parse::<i32>()
            .map_err(|_| "Multiplication high end must be a whole number.")?;

        let config = GameConfig { add_max, mul_max };
        config.validate()?;
        Ok(config)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut terminal = init_terminal()?;
    let result = run(&mut terminal);
    restore_terminal(&mut terminal)?;

    match result {
        Ok(Some(app)) => {
            println!("Time's up!");
            println!("Solved: {}", app.solved);
            Ok(())
        }
        Ok(None) => {
            println!("Canceled.");
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<Option<App>, Box<dyn Error>> {
    let config = match run_setup(terminal)? {
        Some(config) => config,
        None => return Ok(None),
    };

    let app = run_game(terminal, config, Duration::from_secs(60))?;
    Ok(Some(app))
}

fn run_setup(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<Option<GameConfig>, Box<dyn Error>> {
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
                Line::from("Four types are enabled: +, -, *, /"),
                Line::from("Subtraction is reverse addition; division is reverse multiplication."),
                Line::from("Use Up/Down (or j/k) to move. Type numbers. Backspace deletes."),
                Line::from("Press Enter on Start (or 's') to begin. Esc cancels."),
                Line::from(""),
                field_line(
                    "Addition range",
                    &format!("{} to {}", ADD_MIN, setup.add_high_input),
                    setup.focus == SetupField::AddHigh,
                ),
                field_line(
                    "Multiplication range",
                    &format!("{} to {}", MUL_MIN, setup.mul_high_input),
                    setup.focus == SetupField::MulHigh,
                ),
                Line::from(""),
                start_line(setup.focus == SetupField::Start),
            ];

            let setup_widget = Paragraph::new(lines).block(
                Block::default()
                    .title("Mental Math Setup")
                    .borders(Borders::ALL),
            );
            frame.render_widget(setup_widget, chunks[0]);

            let status = Paragraph::new(setup.message.as_str()).block(
                Block::default()
                    .title("Status")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
            frame.render_widget(status, chunks[1]);
        })?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => setup.focus = setup.focus.prev(),
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                        setup.focus = setup.focus.next()
                    }
                    KeyCode::Backspace => {
                        if let Some(input) = setup.active_input_mut() {
                            input.pop();
                        }
                    }
                    KeyCode::Char(c) if c.is_ascii_digit() => {
                        if let Some(input) = setup.active_input_mut() {
                            input.push(c);
                        }
                    }
                    KeyCode::Enter => {
                        if setup.focus == SetupField::Start {
                            match setup.parse_config() {
                                Ok(config) => return Ok(Some(config)),
                                Err(msg) => setup.message = msg.to_string(),
                            }
                        } else {
                            setup.focus = setup.focus.next();
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
                    Constraint::Length(5),
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
