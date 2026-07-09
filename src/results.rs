use std::error::Error;
use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::model::{ADD_MIN, App, MUL_MIN};

pub enum ResultsAction {
    Restart,
    Exit,
}

pub struct RecentAttempt {
    pub score: usize,
}

pub fn run_results(
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
                    let mut spans = vec![
                        Span::raw(format!("{:>3}. {:<18}  ", idx + 1, record.prompt)),
                        Span::styled(format_elapsed(record.elapsed), time_style),
                    ];
                    if let Some(latency) = record.voice_latency {
                        spans.push(Span::styled(
                            format!("  voice {}ms", latency.as_millis()),
                            Style::default().fg(Color::Magenta),
                        ));
                    }
                    history_lines.push(Line::from(spans));
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
