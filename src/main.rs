use std::error::Error;
use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use rand::Rng;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

#[derive(Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
}

struct Question {
    prompt: String,
    answer: i32,
}

struct QuestionGenerator {
    rng: rand::rngs::ThreadRng,
}

impl QuestionGenerator {
    fn new() -> Self {
        Self { rng: rand::rng() }
    }

    fn next(&mut self) -> Question {
        let op = match self.rng.random_range(0..3) {
            0 => Op::Add,
            1 => Op::Sub,
            _ => Op::Mul,
        };

        let (a, b) = match op {
            Op::Add => (
                self.rng.random_range(10..100),
                self.rng.random_range(10..100),
            ),
            Op::Sub => {
                let a = self.rng.random_range(20..100);
                let b = self.rng.random_range(10..=a);
                (a, b)
            }
            Op::Mul => (
                self.rng.random_range(3..13),
                self.rng.random_range(3..13),
            ),
        };

        let (symbol, answer) = match op {
            Op::Add => ("+", a + b),
            Op::Sub => ("-", a - b),
            Op::Mul => ("*", a * b),
        };

        Question {
            prompt: format!("{} {} {} = ?", a, symbol, b),
            answer,
        }
    }
}

struct App {
    generator: QuestionGenerator,
    current: Question,
    input: String,
    score: usize,
    total_answered: usize,
    duration: Duration,
    started_at: Instant,
}

impl App {
    fn new(duration: Duration) -> Self {
        let mut generator = QuestionGenerator::new();
        let current = generator.next();

        Self {
            generator,
            current,
            input: String::new(),
            score: 0,
            total_answered: 0,
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

    fn submit(&mut self) {
        if self.input.trim().is_empty() {
            return;
        }

        if let Ok(value) = self.input.trim().parse::<i32>() {
            self.total_answered += 1;
            if value == self.current.answer {
                self.score += 1;
            }
            self.current = self.generator.next();
        }

        self.input.clear();
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut terminal = init_terminal()?;
    let app_result = run_app(&mut terminal, Duration::from_secs(60));
    restore_terminal(&mut terminal)?;

    match app_result {
        Ok(app) => {
            println!("Time's up!");
            println!("Score: {}/{}", app.score, app.total_answered);
            if app.total_answered > 0 {
                let accuracy = (app.score as f64 / app.total_answered as f64) * 100.0;
                println!("Accuracy: {:.1}%", accuracy);
            }
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    duration: Duration,
) -> Result<App, Box<dyn Error>> {
    let mut app = App::new(duration);

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
                    Constraint::Min(3),
                ])
                .split(area);

            let timer = app.remaining().as_secs();
            let header = Paragraph::new(Line::from(vec![
                Span::raw("Time: "),
                Span::styled(
                    format!("{}s", timer),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::raw("    Score: "),
                Span::styled(
                    format!("{}/{}", app.score, app.total_answered),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ]))
            .block(Block::default().title("Mental Math").borders(Borders::ALL));
            frame.render_widget(header, chunks[0]);

            let question = Paragraph::new(app.current.prompt.clone())
                .block(Block::default().title("Question").borders(Borders::ALL))
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::BOLD));
            frame.render_widget(question, chunks[1]);

            let input = Paragraph::new(app.input.clone()).block(
                Block::default()
                    .title("Answer (Enter to submit, Esc to quit)")
                    .borders(Borders::ALL),
            );
            frame.render_widget(input, chunks[2]);

            let help = Paragraph::new(vec![
                Line::from("Fast mental math drill prototype."),
                Line::from("Only numeric answers are accepted."),
            ])
            .block(Block::default().title("Help").borders(Borders::ALL));
            frame.render_widget(help, chunks[3]);
        })?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char(c) if c.is_ascii_digit() => app.input.push(c),
                    KeyCode::Char('-') if app.input.is_empty() => app.input.push('-'),
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Enter => {
                        app.submit();
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
