use std::error::Error;
use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use tui_big_text::{BigText, PixelSize};

use crate::model::{App, GameConfig};
use crate::voice::VoiceEngine;

pub fn run_game(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: GameConfig,
    duration: Duration,
    use_small_text: bool,
    voice: Option<&VoiceEngine>,
    voice_check: bool,
) -> Result<App, Box<dyn Error>> {
    let mut app = App::new(config, duration);
    let mut voice_latencies: Vec<Duration> = Vec::new();

    // Discard any recognition results left over from a previous round.
    if let Some(engine) = voice {
        while engine.events.try_recv().is_ok() {}
    }

    // With voice on, poll faster so a recognition result isn't stuck waiting
    // out the keyboard poll (part of the speech-to-answer latency budget).
    let poll_timeout = Duration::from_millis(if voice.is_some() { 15 } else { 50 });

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
            let mut header_spans = vec![
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
            ];
            if voice_check {
                if let Some(last) = voice_latencies.last() {
                    let avg = voice_latencies.iter().sum::<Duration>().as_millis()
                        / voice_latencies.len() as u128;
                    header_spans.push(Span::raw("    Voice: "));
                    header_spans.push(Span::styled(
                        format!("{}ms (avg {}ms)", last.as_millis(), avg),
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
            let header = Paragraph::new(Line::from(header_spans))
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

            let input_title = if voice.is_some() {
                "Answer (Esc to quit, voice on)"
            } else {
                "Answer (Esc to quit)"
            };
            let input = Paragraph::new(app.input.clone()).block(
                Block::default().title(input_title).borders(Borders::ALL),
            );
            frame.render_widget(input, chunks[2]);
        })?;

        if event::poll(poll_timeout)? {
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

        if let Some(engine) = voice {
            while let Ok(event) = engine.events.try_recv() {
                app.input = event.value.to_string();
                let score_before = app.score;
                app.try_advance_if_correct();
                if voice_check && app.score > score_before {
                    let latency = event.spoke_at.elapsed();
                    voice_latencies.push(latency);
                    if let Some(record) = app.history.last_mut() {
                        record.voice_latency = Some(latency);
                    }
                }
            }
        }
    }

    Ok(app)
}
