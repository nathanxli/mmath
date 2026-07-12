use std::error::Error;
use std::io;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEventKind,
};
use crossterm::execute;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use tui_big_text::{BigText, PixelSize};

use crate::model::{App, GameConfig};

pub fn run_game(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: GameConfig,
    duration: Duration,
    use_small_text: bool,
    mult_choice: bool,
    wrong_penalty: i32,
) -> Result<App, Box<dyn Error>> {
    let mut app = App::new(config, duration, mult_choice, wrong_penalty);
    // Capture the mouse only while a multiple-choice game runs so terminal
    // text selection keeps working everywhere else.
    if mult_choice {
        execute!(io::stdout(), EnableMouseCapture)?;
    }
    // Grid cell areas from the last frame, for click hit-testing.
    let mut option_rects: [Rect; 4] = [Rect::default(); 4];

    while !app.is_done() {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(6),
                    Constraint::Length(if mult_choice { 12 } else { 3 }),
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
            if let Some(limit) = app.question_limit {
                header_spans.push(Span::raw("    Question: "));
                header_spans.push(Span::styled(
                    format!("{}/{}", app.history.len() + 1, limit),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            let header = Paragraph::new(Line::from(header_spans)).block(
                Block::default()
                    .title(app.config.mode.title())
                    .borders(Borders::ALL),
            );
            frame.render_widget(header, chunks[0]);

            let question_block = Block::default()
                .title("Question")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Reset))
                .style(Style::default().fg(Color::Reset));
            let question_inner = question_block.inner(chunks[1]);
            frame.render_widget(question_block, chunks[1]);

            // Big glyphs are 8 columns per character; long prompts (sequences)
            // fall back to plain text rather than overflowing the box.
            let big_fits =
                app.current.prompt.chars().count() as u16 * 8 <= question_inner.width;
            if use_small_text || !big_fits {
                let question = Paragraph::new(format!("\n{}", app.current.prompt))
                    .alignment(Alignment::Center)
                    .style(Style::default().add_modifier(Modifier::BOLD));
                frame.render_widget(question, question_inner);
            } else {
                let question = BigText::builder()
                    .pixel_size(PixelSize::HalfHeight)
                    .centered()
                    .style(Style::default().add_modifier(Modifier::BOLD))
                    .lines(vec![Line::from(app.current.prompt.clone())])
                    .build();
                frame.render_widget(question, question_inner);
            }

            if let Some(options) = &app.current.options {
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(6), Constraint::Length(6)])
                    .split(chunks[2]);
                for (idx, value) in options.iter().enumerate() {
                    let halves = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .split(rows[idx / 2]);
                    let cell_area = halves[idx % 2];
                    option_rects[idx] = cell_area;
                    let style = Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD);
                    let cell_block = Block::default()
                        .title(format!("{})", idx + 1))
                        .borders(Borders::ALL)
                        .border_style(style);
                    let cell_inner = cell_block.inner(cell_area);
                    let value_fits = value.chars().count() as u16 * 8 <= cell_inner.width;
                    if use_small_text || !value_fits {
                        // Leading blank line to roughly center the value
                        // vertically in the taller cell.
                        let cell = Paragraph::new(format!("\n{}", value))
                            .alignment(Alignment::Center)
                            .style(style)
                            .block(cell_block);
                        frame.render_widget(cell, cell_area);
                    } else {
                        frame.render_widget(cell_block, cell_area);
                        let big_value = BigText::builder()
                            .pixel_size(PixelSize::HalfHeight)
                            .centered()
                            .style(style.add_modifier(Modifier::BOLD))
                            .lines(vec![Line::from(value.clone())])
                            .build();
                        frame.render_widget(big_value, cell_inner);
                    }
                }
            } else {
                let input = Paragraph::new(app.input.clone()).block(
                    Block::default()
                        .title("Answer (Esc to quit)")
                        .borders(Borders::ALL),
                );
                frame.render_widget(input, chunks[2]);
            }
        })?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press && mult_choice => {
                    match key.code {
                        KeyCode::Char(c @ '1'..='4') => {
                            app.answer_with_option(c as usize - '1' as usize);
                        }
                        KeyCode::Esc => break,
                        _ => {}
                    }
                }
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
                Event::Mouse(mouse)
                    if mult_choice
                        && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) =>
                {
                    if let Some(idx) = option_rects
                        .iter()
                        .position(|rect| rect.contains(Position::new(mouse.column, mouse.row)))
                    {
                        app.answer_with_option(idx);
                    }
                }
                _ => {}
            }
        }
    }

    if mult_choice {
        execute!(io::stdout(), DisableMouseCapture)?;
    }
    Ok(app)
}
