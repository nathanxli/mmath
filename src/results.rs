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

/// Population mean and standard deviation; `None` for an empty sample.
fn mean_stdev(values: &[f64]) -> Option<(f64, f64)> {
    if values.is_empty() {
        return None;
    }
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count;
    Some((mean, variance.sqrt()))
}

/// `recent_scores` holds the scores of this session's attempts, oldest first.
pub fn run_results(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &App,
    recent_scores: &[i32],
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
                .constraints([
                    Constraint::Min(6),
                    Constraint::Length(if app.mult_choice { 5 } else { 4 }),
                ])
                .split(middle_chunks[0]);
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(6), Constraint::Length(4)])
                .split(middle_chunks[1]);

            let times: Vec<f64> = app
                .history
                .iter()
                .map(|record| record.elapsed.as_secs_f64())
                .collect();
            let time_stats = mean_stdev(&times);

            let mut history_lines = Vec::new();
            if let Some((mean, stdev)) = time_stats {
                // Flag answers that took unusually long for this session.
                let threshold = mean + (2.0 * stdev);
                for (idx, record) in app.history.iter().enumerate() {
                    let elapsed = record.elapsed.as_secs_f64();
                    let time_style = if elapsed > threshold {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    // Fixed-width columns so times line up across rows,
                    // including rows without voice data.
                    let mut spans = vec![
                        Span::raw(format!("{:>3}. {:<18}  ", idx + 1, record.prompt)),
                        Span::styled(format!("{:>7}", format_elapsed(record.elapsed)), time_style),
                    ];
                    if app.mult_choice {
                        let (mark, color) = if record.correct {
                            ("✓", Color::Green)
                        } else {
                            ("✗", Color::Red)
                        };
                        spans.push(Span::styled(
                            format!("  {}", mark),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ));
                    }
                    if let Some(latency) = record.voice_latency {
                        spans.push(Span::styled(
                            format!("   voice {:>4}ms", latency.as_millis()),
                            Style::default().fg(Color::Magenta),
                        ));
                        spans.push(Span::styled(
                            format!(
                                "   adj {:>7}",
                                format_elapsed(record.elapsed.saturating_sub(latency))
                            ),
                            Style::default().fg(Color::Cyan),
                        ));
                    }
                    history_lines.push(Line::from(spans));
                }
            } else {
                history_lines.push(Line::from("No answered questions."));
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

            let question_stats_lines = match time_stats {
                None => vec![Line::from("μ: n/a"), Line::from("σ: n/a")],
                Some((mean, stdev)) => {
                    let mut lines = vec![
                        Line::from(format!("μ: {:.2}s", mean)),
                        Line::from(format!("σ: {:.2}s", stdev)),
                    ];
                    if app.mult_choice {
                        let correct = app.history.iter().filter(|r| r.correct).count();
                        lines.push(Line::from(format!(
                            "Correct: {}/{}",
                            correct,
                            app.history.len()
                        )));
                    }
                    lines
                }
            };
            let question_stats_widget = Paragraph::new(question_stats_lines).block(
                Block::default()
                    .title("Time per Question")
                    .borders(Borders::ALL)
                    .padding(Padding::left(1)),
            );
            frame.render_widget(question_stats_widget, left_chunks[1]);

            let mut recent_lines = Vec::new();
            if recent_scores.is_empty() {
                recent_lines.push(Line::from("No attempts yet."));
            } else {
                recent_lines.push(Line::from("Scores:"));
                let best = recent_scores.iter().max().copied().unwrap_or(0);
                let worst = recent_scores.iter().min().copied().unwrap_or(0);

                for (idx, &score) in recent_scores.iter().rev().enumerate() {
                    let style = if recent_scores.len() == 1 || score == best {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else if score == worst {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else if idx == 0 {
                        Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    recent_lines.push(Line::from(Span::styled(
                        format!("{:>2}. {}", idx + 1, score),
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

            let scores: Vec<f64> = recent_scores.iter().map(|&s| s as f64).collect();
            let session_stats_lines = match mean_stdev(&scores) {
                None => vec![Line::from("μ: n/a"), Line::from("σ: n/a")],
                Some((mean, stdev)) => vec![
                    Line::from(format!("μ: {:.2}", mean)),
                    Line::from(format!("σ: {:.2}", stdev)),
                ],
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
