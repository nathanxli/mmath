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

use crate::model::{
    ADD_MIN, GameConfig, GameMode, MUL_MIN, TABLE_MAX, default_table_factor_max,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SetupField {
    ModeMentalMath,
    ModeMultTable,
    ModeSequences,
    ModeOptiver,
    AddMode,
    SubMode,
    MulMode,
    DivMode,
    AddHigh,
    MulHighLeft,
    MulHighRight,
    /// Toggle for number `i + 1` in the multiplication table range grid.
    TableNum(usize),
    /// Button that turns every multiplication table number on at once.
    TableSelectAll,
    /// Numeric upper bound for the other factor n in multiplication table mode.
    TableFactorMax,
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
    /// Seeds the multiple-choice default for every non-Optiver mode (`-m`).
    mult_choice_default: bool,
    add_enabled: bool,
    sub_enabled: bool,
    mul_enabled: bool,
    div_enabled: bool,
    table_numbers: [bool; TABLE_MAX],
    table_factor_max_input: String,
    /// True once the user has typed a factor bound, which stops it from
    /// auto-tracking the selected numbers.
    table_factor_max_edited: bool,
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
    pub fn new(mult_choice_default: bool) -> Self {
        let mut state = Self {
            focus: SetupField::ModeMentalMath,
            mode: GameMode::MentalMath,
            mult_choice_default,
            add_enabled: true,
            sub_enabled: true,
            mul_enabled: true,
            div_enabled: true,
            table_numbers: [false; TABLE_MAX],
            table_factor_max_input: String::new(),
            table_factor_max_edited: false,
            add_high_input: String::new(),
            add_high_edited: false,
            mul_high_left_input: String::new(),
            mul_high_left_edited: false,
            mul_high_right_input: String::new(),
            mul_high_right_edited: false,
            time_input: String::new(),
            time_edited: false,
            mult_choice: false,
            penalize_wrong: false,
            large_text: false,
            message: String::from("Set ranges and time, then start."),
        };
        state.apply_mode_defaults();
        state
    }

    /// Reset every option to the current gamemode's default. Called on entry and
    /// whenever the gamemode changes, so options never carry over edits that
    /// were made for a different mode.
    fn apply_mode_defaults(&mut self) {
        let optiver = self.mode == GameMode::Optiver80;
        self.add_enabled = true;
        self.sub_enabled = true;
        self.mul_enabled = true;
        self.div_enabled = true;
        self.table_numbers = [false; TABLE_MAX];
        self.table_factor_max_edited = false;
        self.table_factor_max_input = default_table_factor_max(&self.table_numbers).to_string();
        self.add_high_input = String::from("100");
        self.add_high_edited = false;
        self.mul_high_left_input = String::from("12");
        self.mul_high_left_edited = false;
        self.mul_high_right_input = String::from("100");
        self.mul_high_right_edited = false;
        self.time_input = match self.mode {
            GameMode::Optiver80 => crate::optiver::DEFAULT_SECONDS.to_string(),
            _ => String::from("120"),
        };
        self.time_edited = false;
        // Optiver is locked to the real test's format: multiple choice with a
        // -1 penalty for a wrong answer.
        self.mult_choice = optiver || self.mult_choice_default;
        self.penalize_wrong = optiver;
        self.large_text = false;
    }

    /// Re-derive the other-factor bound from the selected numbers, unless the
    /// user has typed their own value. Called whenever a number toggles.
    fn sync_table_factor_max(&mut self) {
        if !self.table_factor_max_edited {
            self.table_factor_max_input = default_table_factor_max(&self.table_numbers).to_string();
        }
    }

    /// The focusable fields for the current gamemode, in navigation order:
    /// gamemode column first, then the visible option fields, then Start.
    fn field_order(&self) -> Vec<SetupField> {
        let mut fields = vec![
            SetupField::ModeMentalMath,
            SetupField::ModeMultTable,
            SetupField::ModeSequences,
            SetupField::ModeOptiver,
            SetupField::TimeSeconds,
        ];
        // Optiver is always multiple choice with a -1 penalty, so those
        // toggles are fixed rather than focusable. The penalty only applies to
        // multiple-choice answers, so it drops out when that is off.
        if self.mode != GameMode::Optiver80 {
            fields.push(SetupField::MultChoice);
            if self.mult_choice {
                fields.push(SetupField::WrongPenalty);
            }
        }
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
        if self.mode == GameMode::MultTable {
            fields.push(SetupField::TableSelectAll);
            fields.extend((0..TABLE_MAX).map(SetupField::TableNum));
            fields.push(SetupField::TableFactorMax);
        }
        fields.push(SetupField::LargeText);
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

    /// Switch gamemode. Every option resets to the new mode's default -- an edit
    /// made for one mode should never leak into another -- and focus falls back
    /// to Start if it landed on a field the new mode doesn't show.
    fn select_mode(&mut self, mode: GameMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.apply_mode_defaults();
        if !self.field_order().contains(&self.focus) {
            self.focus = SetupField::Start;
        }
        self.message = format!("Gamemode: {}.", mode.title());
    }

    fn active_input_mut(&mut self) -> Option<(&mut String, &mut bool)> {
        match self.focus {
            SetupField::ModeMentalMath => None,
            SetupField::ModeMultTable => None,
            SetupField::ModeSequences => None,
            SetupField::ModeOptiver => None,
            SetupField::AddMode => None,
            SetupField::SubMode => None,
            SetupField::MulMode => None,
            SetupField::DivMode => None,
            SetupField::TableNum(_) => None,
            SetupField::TableSelectAll => None,
            SetupField::TableFactorMax => Some((
                &mut self.table_factor_max_input,
                &mut self.table_factor_max_edited,
            )),
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
            SetupField::ModeMultTable => {
                self.select_mode(GameMode::MultTable);
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
            SetupField::TableNum(i) => {
                self.table_numbers[i] = !self.table_numbers[i];
                self.sync_table_factor_max();
                true
            }
            SetupField::TableSelectAll => {
                // Turn everything on, or off again once it is all on.
                let all_on = self.table_numbers.iter().all(|&on| on);
                self.table_numbers = [!all_on; TABLE_MAX];
                self.sync_table_factor_max();
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
                    table_numbers: [true; TABLE_MAX],
                    table_factor_max: 10,
                };
                config.validate()?;
                config
            }
            GameMode::MultTable => {
                let table_factor_max = self
                    .table_factor_max_input
                    .parse::<i32>()
                    .map_err(|_| "Other factor high end must be a whole number.")?;
                let config = GameConfig {
                    mode: self.mode,
                    add_max: 100,
                    mul_max_left: 12,
                    mul_max_right: 100,
                    add_enabled: true,
                    sub_enabled: true,
                    mul_enabled: true,
                    div_enabled: true,
                    table_numbers: self.table_numbers,
                    table_factor_max,
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
                table_numbers: [true; TABLE_MAX],
                table_factor_max: 10,
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
                    Constraint::Min(23),    // single-column body
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
        (GameMode::MultTable, SetupField::ModeMultTable),
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

/// Right column: the options for the selected gamemode. Time, Multiple choice
/// and Text are always present; Operations and Ranges are Mental Math only.
fn render_options_column(
    frame: &mut ratatui::Frame,
    area: Rect,
    setup: &SetupState,
    click_targets: &mut Vec<(Rect, SetupField)>,
) {
    let mental_math = setup.mode == GameMode::MentalMath;
    let mult_table = setup.mode == GameMode::MultTable;
    let optiver = setup.mode == GameMode::Optiver80;

    let mut constraints = vec![
        Constraint::Length(3), // Time
        Constraint::Length(4), // Multiple choice (toggle + penalty)
    ];
    if mental_math {
        constraints.push(Constraint::Length(8)); // Operations
        constraints.push(Constraint::Length(5)); // Ranges
    }
    if mult_table {
        constraints.push(Constraint::Length(13)); // Range (button + number grid)
    }
    constraints.push(Constraint::Length(3)); // Text
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

    // --- Multiple choice, with the wrong-answer penalty that only applies to
    // it. Both are fixed on for Optiver, so neither is clickable there. ---
    let mult_block = Block::default()
        .title("Multiple choice")
        .borders(Borders::ALL)
        .padding(Padding::left(1));
    let mult_inner = mult_block.inner(rows[row]);
    frame.render_widget(mult_block, rows[row]);

    let mult_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(mult_inner);

    let dim = Style::default().fg(Color::DarkGray);
    let (mult_value, penalty_value) = if optiver {
        (
            Span::styled("always on", dim),
            Span::styled("always on", dim),
        )
    } else {
        (
            mode_span("", setup.mult_choice, setup.focus == SetupField::MultChoice),
            penalty_span(
                setup.penalize_wrong,
                setup.focus == SetupField::WrongPenalty,
                setup.mult_choice,
            ),
        )
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::raw("Enabled     "), mult_value])),
        mult_rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "Penalty −1  ",
                if optiver || setup.mult_choice {
                    Style::default()
                } else {
                    dim
                },
            ),
            penalty_value,
        ])),
        mult_rows[1],
    );
    if !optiver {
        click_targets.push((mult_rows[0], SetupField::MultChoice));
        // The penalty is inert without multiple choice, so it is not clickable
        // then either -- matching its "[n/a]" display.
        if setup.mult_choice {
            click_targets.push((mult_rows[1], SetupField::WrongPenalty));
        }
    }
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

    if mult_table {
        // --- Range: 3x5 clickable grid of the numbers 1-15, mirroring the
        // Operations grid. ---
        let range_block = Block::default().title("Range").borders(Borders::ALL);
        let range_inner = range_block.inner(rows[row]);
        frame.render_widget(range_block, rows[row]);
        row += 1;

        let range_sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // select/deselect all button
                Constraint::Length(9), // number grid
                Constraint::Length(1), // other-factor bound
            ])
            .split(range_inner);

        // --- Select/deselect all: turns every number on, or off once all on. ---
        let all_on = setup.table_numbers.iter().all(|&on| on);
        let focused = setup.focus == SetupField::TableSelectAll;
        let mut all_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        if focused {
            all_style = all_style.add_modifier(Modifier::UNDERLINED);
        }
        let all_label = if all_on {
            "[ Deselect all ]"
        } else {
            "[ Select all ]"
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(all_label, all_style)))
                .alignment(Alignment::Center),
            range_sections[0],
        );
        click_targets.push((range_sections[0], SetupField::TableSelectAll));

        let range_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
            ])
            .split(range_sections[1]);

        for i in 0..TABLE_MAX {
            let cells = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Ratio(1, 5); 5])
                .split(range_rows[i / 5]);
            let cell_area = cells[i % 5];
            let field = SetupField::TableNum(i);
            let enabled = setup.table_numbers[i];
            let focused = setup.focus == field;

            let border_color = if enabled { Color::Green } else { Color::Red };
            let cell_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color));
            let cell_inner = cell_block.inner(cell_area);
            frame.render_widget(cell_block, cell_area);

            let mut style = if enabled {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            };
            if focused {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled((i + 1).to_string(), style)))
                    .alignment(Alignment::Center),
                cell_inner,
            );
            click_targets.push((cell_area, field));
        }

        // --- Other factor n: MUL_MIN..=this bound. Tracks the selected
        // numbers until the user types a value of their own. ---
        frame.render_widget(
            Paragraph::new(range_line(
                "Other factor upper bound ",
                MUL_MIN,
                setup.table_factor_max_input.as_str(),
                setup.focus == SetupField::TableFactorMax,
            )),
            range_sections[2],
        );
        click_targets.push((range_sections[2], SetupField::TableFactorMax));
    }

    // --- Text. ---
    let text_block = Block::default()
        .title("Text")
        .borders(Borders::ALL)
        .padding(Padding::left(1));
    let text_inner = text_block.inner(rows[row]);
    frame.render_widget(text_block, rows[row]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("Large text  "),
            mode_span("", setup.large_text, setup.focus == SetupField::LargeText),
        ])),
        text_inner,
    );
    click_targets.push((rows[row], SetupField::LargeText));
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

/// The wrong-answer penalty. It can only bite on a multiple-choice answer, so
/// without multiple choice it reads as an inert "[n/a]" rather than on/off.
fn penalty_span(enabled: bool, focused: bool, mult_choice_on: bool) -> Span<'static> {
    if mult_choice_on {
        mode_span("", enabled, focused)
    } else {
        Span::styled("[n/a]", Style::default().fg(Color::DarkGray))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Focus the gamemode's cell and activate it, as a click or Space would.
    fn switch_to(state: &mut SetupState, field: SetupField) {
        state.focus = field;
        state.toggle_focused_mode();
    }

    #[test]
    fn sequences_mode_hides_mental_math_fields() {
        let mut state = SetupState::new(false);
        switch_to(&mut state, SetupField::ModeSequences);
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
    fn mult_table_shows_number_toggles_and_defaults_all_off() {
        let mut state = SetupState::new(false);
        switch_to(&mut state, SetupField::ModeMultTable);
        let order = state.field_order();
        assert!(!order.contains(&SetupField::AddMode));
        assert!(!order.contains(&SetupField::AddHigh));
        assert!(order.contains(&SetupField::TableSelectAll));
        assert!(order.contains(&SetupField::TableFactorMax));
        for i in 0..TABLE_MAX {
            assert!(order.contains(&SetupField::TableNum(i)));
        }
        assert!(!state.mult_choice, "multiple choice defaults off");
        // Every number starts off, so a fresh mult table config cannot start.
        assert!(state.table_numbers.iter().all(|&on| !on));
        assert!(state.parse_config().is_err());
    }

    #[test]
    fn mult_table_factor_bound_tracks_selection_until_manually_edited() {
        let mut state = SetupState::new(false);
        switch_to(&mut state, SetupField::ModeMultTable);
        // No numbers selected: bound sits at the floor of 10.
        assert_eq!(state.table_factor_max_input, "10");
        // Selecting a small table keeps the floor; a big one lifts the bound.
        switch_to(&mut state, SetupField::TableNum(2)); // 3
        assert_eq!(state.table_factor_max_input, "10");
        switch_to(&mut state, SetupField::TableNum(13)); // 14
        assert_eq!(state.table_factor_max_input, "14");

        // Typing a value freezes it: further toggles no longer move it.
        state.focus = SetupField::TableFactorMax;
        for c in "20".chars() {
            let (input, edited) = state.active_input_mut().unwrap();
            if !*edited {
                input.clear();
                *edited = true;
            }
            input.push(c);
        }
        switch_to(&mut state, SetupField::TableNum(14)); // 15
        assert_eq!(state.table_factor_max_input, "20");
        let config = state.parse_config().expect("edited bound parses");
        assert_eq!(config.game.table_factor_max, 20);
    }

    #[test]
    fn mult_table_switching_modes_resets_the_factor_bound() {
        let mut state = SetupState::new(false);
        switch_to(&mut state, SetupField::ModeMultTable);
        state.focus = SetupField::TableFactorMax;
        state.table_factor_max_input = String::from("99");
        state.table_factor_max_edited = true;
        // Leaving and returning restores the auto-tracking default.
        switch_to(&mut state, SetupField::ModeMentalMath);
        switch_to(&mut state, SetupField::ModeMultTable);
        assert_eq!(state.table_factor_max_input, "10");
        assert!(!state.table_factor_max_edited);
    }

    #[test]
    fn mult_table_select_all_toggles_between_all_on_and_all_off() {
        let mut state = SetupState::new(false);
        switch_to(&mut state, SetupField::ModeMultTable);
        // First activation selects every number...
        switch_to(&mut state, SetupField::TableSelectAll);
        assert!(state.table_numbers.iter().all(|&on| on));
        let config = state.parse_config().expect("all numbers selected");
        assert!(config.game.mode == GameMode::MultTable);
        assert!(config.game.table_numbers.iter().all(|&on| on));
        // ...a second activation deselects them all.
        switch_to(&mut state, SetupField::TableSelectAll);
        assert!(state.table_numbers.iter().all(|&on| !on));
        assert!(state.parse_config().is_err());
    }

    #[test]
    fn mult_table_requires_at_least_one_number() {
        let mut state = SetupState::new(false);
        switch_to(&mut state, SetupField::ModeMultTable);
        assert!(state.parse_config().is_err());
        switch_to(&mut state, SetupField::TableNum(4));
        let config = state.parse_config().expect("one number is enough");
        assert!(config.game.table_numbers[4]);
        assert_eq!(config.game.table_numbers.iter().filter(|&&on| on).count(), 1);
    }

    #[test]
    fn defaults_are_typed_text_and_single_answer() {
        let state = SetupState::new(false);
        assert!(!state.large_text);
        assert!(!state.mult_choice);
        assert!(!state.penalize_wrong);
        let config = state.parse_config().expect("default config should parse");
        assert!(!config.large_text);
        assert!(!config.mult_choice);
    }

    #[test]
    fn penalty_is_only_reachable_with_multiple_choice_on() {
        let mut state = SetupState::new(false);
        assert!(!state.mult_choice);
        assert!(!state.field_order().contains(&SetupField::WrongPenalty));

        switch_to(&mut state, SetupField::MultChoice);
        assert!(state.mult_choice);
        assert!(state.field_order().contains(&SetupField::WrongPenalty));
    }

    #[test]
    fn optiver_forces_mult_choice_penalty_and_test_duration() {
        let mut state = SetupState::new(false);
        switch_to(&mut state, SetupField::ModeOptiver);
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
    fn switching_gamemode_restores_that_modes_defaults() {
        let mut state = SetupState::new(false);
        // Edit a spread of options under Mental Math...
        state.time_input = String::from("60");
        state.time_edited = true;
        state.add_high_input = String::from("999");
        state.add_high_edited = true;
        state.mul_enabled = false;
        state.mult_choice = true;
        state.large_text = true;

        // ...then take the Optiver detour, which has its own defaults...
        switch_to(&mut state, SetupField::ModeOptiver);
        assert_eq!(state.time_input, "480");
        assert!(state.mult_choice);
        assert!(state.penalize_wrong);
        assert!(!state.large_text);

        // ...and back: none of the earlier edits may leak through.
        switch_to(&mut state, SetupField::ModeMentalMath);
        assert_eq!(state.time_input, "120");
        assert_eq!(state.add_high_input, "100");
        assert!(state.mul_enabled);
        assert!(!state.mult_choice);
        assert!(!state.penalize_wrong);
        assert!(!state.large_text);
    }

    #[test]
    fn reselecting_the_current_gamemode_keeps_edits() {
        let mut state = SetupState::new(false);
        state.time_input = String::from("60");
        state.time_edited = true;
        switch_to(&mut state, SetupField::ModeMentalMath);
        assert_eq!(state.time_input, "60");
    }

    #[test]
    fn mult_choice_flag_seeds_every_non_optiver_mode() {
        let mut state = SetupState::new(true);
        assert!(state.mult_choice);
        switch_to(&mut state, SetupField::ModeSequences);
        assert!(state.mult_choice);
    }

    #[test]
    fn sequences_start_ignores_range_inputs() {
        let mut state = SetupState::new(false);
        switch_to(&mut state, SetupField::ModeSequences);
        // A range input left in an invalid state must not block other modes.
        state.add_high_input = String::new();
        let config = state
            .parse_config()
            .expect("sequences must not validate mental math ranges");
        assert!(config.game.mode == GameMode::Sequences);
    }
}
