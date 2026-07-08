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

use crate::model::{ADD_MIN, GameConfig, MUL_MIN};

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

pub struct SetupConfig {
    pub game: GameConfig,
    pub duration: Duration,
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

pub fn run_setup(
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
