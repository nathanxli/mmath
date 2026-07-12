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
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::model::{ADD_MIN, GameConfig, GameMode, MUL_MIN};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SetupField {
    ModeMentalMath,
    ModeSequences,
    ModeOptiver,
    AddMode,
    SubMode,
    MulMode,
    DivMode,
    AddHigh,
    MulHighLeft,
    MulHighRight,
    TimeSeconds,
    LargeText,
    MultChoice,
    WrongPenalty,
    Start,
}

pub struct SetupConfig {
    pub game: GameConfig,
    pub duration: Duration,
    pub mult_choice: bool,
    pub wrong_penalty: i32,
    pub large_text: bool,
}

pub struct SetupState {
    focus: SetupField,
    mode: GameMode,
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
    mult_choice: bool,
    penalize_wrong: bool,
    large_text: bool,
    message: String,
}

impl SetupState {
    pub fn new(mult_choice_default: bool, large_text_default: bool) -> Self {
        Self {
            focus: SetupField::ModeMentalMath,
            mode: GameMode::MentalMath,
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
            mult_choice: mult_choice_default,
            penalize_wrong: false,
            large_text: large_text_default,
            message: String::from("Set ranges and time, then start."),
        }
    }

    /// The focusable fields for the current gamemode, in navigation order:
    /// gamemode column first, then the visible option fields, then Start.
    fn field_order(&self) -> Vec<SetupField> {
        let mut fields = vec![
            SetupField::ModeMentalMath,
            SetupField::ModeSequences,
            SetupField::ModeOptiver,
            SetupField::TimeSeconds,
        ];
        if self.mode == GameMode::MentalMath {
            fields.extend([
                SetupField::AddMode,
                SetupField::SubMode,
                SetupField::MulMode,
                SetupField::DivMode,
                SetupField::AddHigh,
                SetupField::MulHighLeft,
                SetupField::MulHighRight,
            ]);
        }
        fields.push(SetupField::LargeText);
        // Optiver is always multiple choice with a -1 penalty, so those
        // toggles are fixed rather than focusable.
        if self.mode != GameMode::Optiver80 {
            fields.extend([SetupField::MultChoice, SetupField::WrongPenalty]);
        }
        fields.push(SetupField::Start);
        fields
    }

    fn focus_next(&mut self) {
        let order = self.field_order();
        let idx = order.iter().position(|&f| f == self.focus).unwrap_or(0);
        self.focus = order[(idx + 1) % order.len()];
    }

    fn focus_prev(&mut self) {
        let order = self.field_order();
        let idx = order.iter().position(|&f| f == self.focus).unwrap_or(0);
        self.focus = order[(idx + order.len() - 1) % order.len()];
    }

    /// Switch gamemode, dropping focus/settings that don't exist in the new
    /// mode.
    fn select_mode(&mut self, mode: GameMode) {
        self.mode = mode;
        if mode == GameMode::Optiver80 {
            self.mult_choice = true;
            self.penalize_wrong = true;
        }
        // Swap in the mode's natural duration unless the user typed one.
        if !self.time_edited {
            self.time_input = match mode {
                GameMode::Optiver80 => crate::optiver::DEFAULT_SECONDS.to_string(),
                _ => String::from("120"),
            };
        }
        if !self.field_order().contains(&self.focus) {
            self.focus = SetupField::Start;
        }
        self.message = format!("Gamemode: {}.", mode.title());
    }

    fn active_input_mut(&mut self) -> Option<(&mut String, &mut bool)> {
        match self.focus {
            SetupField::ModeMentalMath => None,
            SetupField::ModeSequences => None,
            SetupField::ModeOptiver => None,
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
            SetupField::LargeText => None,
            SetupField::MultChoice => None,
            SetupField::WrongPenalty => None,
            SetupField::Start => None,
        }
    }

    fn toggle_focused_mode(&mut self) -> bool {
        match self.focus {
            SetupField::ModeMentalMath => {
                self.select_mode(GameMode::MentalMath);
                true
            }
            SetupField::ModeSequences => {
                self.select_mode(GameMode::Sequences);
                true
            }
            SetupField::ModeOptiver => {
                self.select_mode(GameMode::Optiver80);
                true
            }
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
            SetupField::LargeText => {
                self.large_text = !self.large_text;
                true
            }
            SetupField::MultChoice => {
                self.mult_choice = !self.mult_choice;
                true
            }
            SetupField::WrongPenalty => {
                self.penalize_wrong = !self.penalize_wrong;
                true
            }
            _ => false,
        }
    }

    fn parse_config(&self) -> Result<SetupConfig, &'static str> {
        let time_seconds = self
            .time_input
            .parse::<u64>()
            .map_err(|_| "Time must be a whole number of seconds.")?;
        if time_seconds == 0 {
            return Err("Time must be at least 1 second.");
        }

        // Operation toggles and ranges only apply to Mental Math; other modes
        // use defaults so a stray range edit can't block starting them.
        let config = match self.mode {
            GameMode::MentalMath => {
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
                let config = GameConfig {
                    mode: self.mode,
                    add_max,
                    mul_max_left,
                    mul_max_right,
                    add_enabled: self.add_enabled,
                    sub_enabled: self.sub_enabled,
                    mul_enabled: self.mul_enabled,
                    div_enabled: self.div_enabled,
                };
                config.validate()?;
                config
            }
            GameMode::Sequences | GameMode::Optiver80 => GameConfig {
                mode: self.mode,
                add_max: 100,
                mul_max_left: 12,
                mul_max_right: 100,
                add_enabled: true,
                sub_enabled: true,
                mul_enabled: true,
                div_enabled: true,
            },
        };
        // Optiver is locked to the real test's format: multiple choice, -1
        // for a wrong answer.
        let optiver = self.mode == GameMode::Optiver80;
        Ok(SetupConfig {
            game: config,
            duration: Duration::from_secs(time_seconds),
            mult_choice: self.mult_choice || optiver,
            wrong_penalty: if self.penalize_wrong || optiver { -1 } else { 0 },
            large_text: self.large_text,
        })
    }
}

/// Takes the state by reference so a caller can re-enter the menu without
/// discarding the ranges the user already typed.
pub fn run_setup(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    setup: &mut SetupState,
) -> Result<Option<SetupConfig>, Box<dyn Error>> {
    // Capture the mouse so setup rows/cells are clickable, mirroring the
    // multiple-choice grid in game.rs. Disable it on every exit path so normal
    // terminal text selection keeps working elsewhere.
    execute!(io::stdout(), EnableMouseCapture)?;
    let outcome = run_setup_loop(terminal, setup);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    outcome
}

fn run_setup_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    setup: &mut SetupState,
) -> Result<Option<SetupConfig>, Box<dyn Error>> {
    loop {
        // Screen regions of clickable controls from the last frame, for click
        // hit-testing. Rebuilt every frame; layout is deterministic so the
        // previous frame's rects are valid for the click that follows.
        let mut click_targets: Vec<(Rect, SetupField)> = Vec::new();

        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),  // help line
                    Constraint::Min(22),    // single-column body
                    Constraint::Length(3),  // Start button
                    Constraint::Length(3),  // status bar
                ])
                .split(area);

            let help = Paragraph::new(
                "Click a control, or use Up/Down (j/k) + Space/Enter. Digits edit ranges. \
                 's' starts, Esc cancels.",
            )
            .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(help, chunks[0]);

            // Gamemode selection on the left; the selected gamemode's options
            // on the right. Start/Status span the full width below.
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(chunks[1]);

            render_gamemode_column(frame, columns[0], setup, &mut click_targets);
            render_options_column(frame, columns[1], setup, &mut click_targets);

            render_start_button(frame, chunks[2], setup.focus == SetupField::Start);
            click_targets.push((chunks[2], SetupField::Start));

            let status = Paragraph::new(setup.message.as_str()).block(
                Block::default()
                    .title("Status")
                    .borders(Borders::ALL)
                    .padding(Padding::left(1))
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
            frame.render_widget(status, chunks[3]);
        })?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => setup.focus_prev(),
                    KeyCode::Down | KeyCode::Char('j') => setup.focus_next(),
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
                Event::Mouse(mouse)
                    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) =>
                {
                    if let Some(&(_, field)) = click_targets
                        .iter()
                        .find(|(rect, _)| rect.contains(Position::new(mouse.column, mouse.row)))
                    {
                        // A click focuses the target. Toggles flip immediately;
                        // Start begins the game; numeric fields just take focus
                        // so the keyboard can edit them.
                        setup.focus = field;
                        if field == SetupField::Start {
                            match setup.parse_config() {
                                Ok(config) => return Ok(Some(config)),
                                Err(msg) => setup.message = msg.to_string(),
                            }
                        } else {
                            setup.toggle_focused_mode();
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Left column: one clickable cell per gamemode; the selected one has a green
/// border.
fn render_gamemode_column(
    frame: &mut ratatui::Frame,
    area: Rect,
    setup: &SetupState,
    click_targets: &mut Vec<(Rect, SetupField)>,
) {
    let modes = [
        (GameMode::MentalMath, SetupField::ModeMentalMath),
        (GameMode::Sequences, SetupField::ModeSequences),
        (GameMode::Optiver80, SetupField::ModeOptiver),
    ];

    let column_block = Block::default().title("Gamemode").borders(Borders::ALL);
    let column_inner = column_block.inner(area);
    frame.render_widget(column_block, area);

    let mut constraints: Vec<Constraint> =
        modes.iter().map(|_| Constraint::Length(5)).collect();
    constraints.push(Constraint::Min(0));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(column_inner);

    for (idx, &(mode, field)) in modes.iter().enumerate() {
        let selected = setup.mode == mode;
        let focused = setup.focus == field;
        let border_color = if selected { Color::Green } else { Color::DarkGray };
        let cell_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));
        let cell_inner = cell_block.inner(rows[idx]);
        frame.render_widget(cell_block, rows[idx]);

        let mut style = if selected {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        if focused {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        // Middle line of the 3-row inner area, to center vertically.
        frame.render_widget(
            Paragraph::new(vec![
                Line::default(),
                Line::from(Span::styled(mode.title(), style)),
            ])
            .alignment(Alignment::Center),
            cell_inner,
        );
        click_targets.push((rows[idx], field));
    }
}

/// Right column: the options for the selected gamemode. Time and the Options
/// toggles are always present; Operations and Ranges are Mental Math only.
fn render_options_column(
    frame: &mut ratatui::Frame,
    area: Rect,
    setup: &SetupState,
    click_targets: &mut Vec<(Rect, SetupField)>,
) {
    let mental_math = setup.mode == GameMode::MentalMath;
    let optiver = setup.mode == GameMode::Optiver80;

    // `None` fields are fixed-format info rows, not clickable toggles.
    let option_lines: Vec<(Line, Option<SetupField>)> = {
        let mut lines = vec![(
            large_text_line(setup.large_text, setup.focus == SetupField::LargeText),
            Some(SetupField::LargeText),
        )];
        if optiver {
            let dim = Style::default().fg(Color::DarkGray);
            lines.push((
                Line::from(Span::styled("Mult choice:  always on", dim)),
                None,
            ));
            lines.push((
                Line::from(Span::styled("Penalty −1:   always on", dim)),
                None,
            ));
        } else {
            lines.push((
                mult_choice_line(setup.mult_choice, setup.focus == SetupField::MultChoice),
                Some(SetupField::MultChoice),
            ));
            lines.push((
                penalty_line(
                    setup.penalize_wrong,
                    setup.focus == SetupField::WrongPenalty,
                    setup.mult_choice,
                ),
                Some(SetupField::WrongPenalty),
            ));
        }
        lines
    };

    let mut constraints = vec![Constraint::Length(3)]; // Time
    if mental_math {
        constraints.push(Constraint::Length(8)); // Operations
        constraints.push(Constraint::Length(5)); // Ranges
    }
    constraints.push(Constraint::Length(option_lines.len() as u16 + 2)); // Options
    constraints.push(Constraint::Min(0));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    let mut row = 0;

    // --- Time. ---
    let time_block = Block::default()
        .title("Time (seconds)")
        .borders(Borders::ALL)
        .padding(Padding::left(1));
    let time_inner = time_block.inner(rows[row]);
    frame.render_widget(time_block, rows[row]);
    frame.render_widget(
        Paragraph::new(value_line(
            setup.time_input.as_str(),
            setup.focus == SetupField::TimeSeconds,
        )),
        time_inner,
    );
    click_targets.push((rows[row], SetupField::TimeSeconds));
    row += 1;

    if mental_math {
        // --- Operations: 2x2 clickable grid, mirroring the answer grid. ---
        let ops_block = Block::default().title("Operations").borders(Borders::ALL);
        let ops_inner = ops_block.inner(rows[row]);
        frame.render_widget(ops_block, rows[row]);
        row += 1;

        let ops_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(3)])
            .split(ops_inner);

        let cells = [
            ("+", setup.add_enabled, SetupField::AddMode),
            ("-", setup.sub_enabled, SetupField::SubMode),
            ("*", setup.mul_enabled, SetupField::MulMode),
            ("/", setup.div_enabled, SetupField::DivMode),
        ];
        for (idx, &(symbol, enabled, field)) in cells.iter().enumerate() {
            let halves = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(ops_rows[idx / 2]);
            let cell_area = halves[idx % 2];
            let focused = setup.focus == field;

            // Border is always green/red for on/off; focus is shown by the
            // underlined inner text (mode_span), never a color change.
            let border_color = if enabled { Color::Green } else { Color::Red };
            let cell_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color));
            let cell_inner = cell_block.inner(cell_area);
            frame.render_widget(cell_block, cell_area);
            frame.render_widget(
                Paragraph::new(Line::from(mode_span(symbol, enabled, focused)))
                    .alignment(Alignment::Center),
                cell_inner,
            );
            click_targets.push((cell_area, field));
        }

        // --- Ranges: one clickable numeric row per field. ---
        let ranges_block = Block::default()
            .title("Ranges")
            .borders(Borders::ALL)
            .padding(Padding::left(1));
        let ranges_inner = ranges_block.inner(rows[row]);
        frame.render_widget(ranges_block, rows[row]);
        row += 1;

        let range_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(ranges_inner);

        let ranges = [
            (
                "Addition",
                ADD_MIN,
                setup.add_high_input.as_str(),
                SetupField::AddHigh,
            ),
            (
                "Mult ×a",
                MUL_MIN,
                setup.mul_high_left_input.as_str(),
                SetupField::MulHighLeft,
            ),
            (
                "Mult ×b",
                MUL_MIN,
                setup.mul_high_right_input.as_str(),
                SetupField::MulHighRight,
            ),
        ];
        for (idx, &(label, low, value, field)) in ranges.iter().enumerate() {
            let focused = setup.focus == field;
            frame.render_widget(
                Paragraph::new(range_line(label, low, value, focused)),
                range_rows[idx],
            );
            click_targets.push((range_rows[idx], field));
        }
    }

    // --- Options: one clickable toggle per row. ---
    let opts_block = Block::default()
        .title("Options")
        .borders(Borders::ALL)
        .padding(Padding::left(1));
    let opts_inner = opts_block.inner(rows[row]);
    frame.render_widget(opts_block, rows[row]);

    let opt_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(1); option_lines.len()])
        .split(opts_inner);

    for (idx, (line, field)) in option_lines.into_iter().enumerate() {
        frame.render_widget(Paragraph::new(line), opt_rows[idx]);
        if let Some(field) = field {
            click_targets.push((opt_rows[idx], field));
        }
    }
}

fn render_start_button(frame: &mut ratatui::Frame, area: Rect, focused: bool) {
    let border_color = if focused { Color::Yellow } else { Color::Green };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    if focused {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("▶  START", style))).alignment(Alignment::Center),
        inner,
    );
}

/// Label + value styles for a numeric field, highlighted when focused.
fn field_styles(focused: bool) -> (Style, Style) {
    if focused {
        (
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
    } else {
        (Style::default(), Style::default())
    }
}

fn range_line(label: &str, low: i32, value: &str, focused: bool) -> Line<'static> {
    let (label_style, value_style) = field_styles(focused);
    Line::from(vec![
        Span::styled(format!("{:<9}{} – ", label, low), label_style),
        Span::styled(format!("[{}]", value), value_style),
    ])
}

fn value_line(value: &str, focused: bool) -> Line<'static> {
    let (_, value_style) = field_styles(focused);
    Line::from(Span::styled(format!("[{}]", value), value_style))
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
    let text = if symbol.is_empty() {
        format!("[{}]", if enabled { "on" } else { "off" })
    } else {
        format!("{} [{}]", symbol, if enabled { "on" } else { "off" })
    };
    Span::styled(text, style)
}

fn large_text_line(enabled: bool, focused: bool) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled("Large text:   ", label_style),
        mode_span("", enabled, focused),
    ])
}

fn mult_choice_line(enabled: bool, focused: bool) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled("Mult choice:  ", label_style),
        mode_span("", enabled, focused),
    ])
}

fn penalty_line(enabled: bool, focused: bool, mult_choice_on: bool) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if !mult_choice_on {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled("Penalty −1:   ", label_style),
        mode_span("", enabled, focused),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_mode_hides_mental_math_fields() {
        let mut state = SetupState::new(false, true);
        state.focus = SetupField::ModeSequences;
        state.toggle_focused_mode();
        let order = state.field_order();
        assert!(!order.contains(&SetupField::AddMode));
        assert!(!order.contains(&SetupField::AddHigh));
        // Navigation wraps through exactly the visible fields.
        for _ in 0..order.len() {
            state.focus_next();
        }
        assert_eq!(state.focus, SetupField::ModeSequences);
    }

    #[test]
    fn optiver_forces_mult_choice_penalty_and_test_duration() {
        let mut state = SetupState::new(false, true);
        state.focus = SetupField::ModeOptiver;
        state.toggle_focused_mode();
        assert_eq!(state.time_input, "480");
        let order = state.field_order();
        assert!(!order.contains(&SetupField::MultChoice));
        assert!(!order.contains(&SetupField::WrongPenalty));
        let config = state.parse_config().expect("optiver config should parse");
        assert!(config.game.mode == GameMode::Optiver80);
        assert!(config.mult_choice);
        assert_eq!(config.wrong_penalty, -1);
        assert_eq!(config.duration.as_secs(), 480);
    }

    #[test]
    fn user_edited_time_survives_mode_switch() {
        let mut state = SetupState::new(false, true);
        state.time_input = String::from("60");
        state.time_edited = true;
        state.focus = SetupField::ModeOptiver;
        state.toggle_focused_mode();
        assert_eq!(state.time_input, "60");
    }

    #[test]
    fn sequences_start_ignores_range_inputs() {
        let mut state = SetupState::new(false, true);
        state.focus = SetupField::ModeSequences;
        state.toggle_focused_mode();
        // A range input left in an invalid state must not block other modes.
        state.add_high_input = String::new();
        let config = state
            .parse_config()
            .expect("sequences must not validate mental math ranges");
        assert!(config.game.mode == GameMode::Sequences);
    }
}
