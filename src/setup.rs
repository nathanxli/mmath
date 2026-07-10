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

use crate::model::{ADD_MIN, GameConfig, MUL_MIN};
use crate::voice;

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
    LargeText,
    Voice,
    MultChoice,
    WrongPenalty,
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
            SetupField::TimeSeconds => SetupField::LargeText,
            SetupField::LargeText => SetupField::Voice,
            SetupField::Voice => SetupField::MultChoice,
            SetupField::MultChoice => SetupField::WrongPenalty,
            SetupField::WrongPenalty => SetupField::Start,
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
            SetupField::LargeText => SetupField::TimeSeconds,
            SetupField::Voice => SetupField::LargeText,
            SetupField::MultChoice => SetupField::Voice,
            SetupField::WrongPenalty => SetupField::MultChoice,
            SetupField::Start => SetupField::WrongPenalty,
        }
    }
}

pub struct SetupConfig {
    pub game: GameConfig,
    pub duration: Duration,
    pub voice_enabled: bool,
    pub mult_choice: bool,
    pub wrong_penalty: i32,
    pub large_text: bool,
}

pub struct SetupState {
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
    voice_enabled: bool,
    mult_choice: bool,
    penalize_wrong: bool,
    large_text: bool,
    message: String,
}

impl SetupState {
    pub fn new(voice_default: bool, mult_choice_default: bool, large_text_default: bool) -> Self {
        let voice_wanted = voice_default && !mult_choice_default;
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
            // Voice and multiple-choice are mutually exclusive; -m wins if
            // both flags are given.
            voice_enabled: voice_wanted && voice::AVAILABLE,
            mult_choice: mult_choice_default,
            penalize_wrong: false,
            large_text: large_text_default,
            message: if voice_wanted && !voice::AVAILABLE {
                String::from(voice::UNSUPPORTED)
            } else {
                String::from("Set ranges and time, then start.")
            },
        }
    }

    /// Voice startup failed at runtime (missing model, no microphone). Drop back
    /// into the menu with the reason rather than killing the session.
    pub fn voice_failed(&mut self, err: String) {
        self.voice_enabled = false;
        self.message = format!("Voice unavailable: {}", err);
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
            SetupField::LargeText => None,
            SetupField::Voice => None,
            SetupField::MultChoice => None,
            SetupField::WrongPenalty => None,
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
            SetupField::LargeText => {
                self.large_text = !self.large_text;
                true
            }
            SetupField::Voice => {
                if !voice::AVAILABLE {
                    self.message = String::from(voice::UNSUPPORTED);
                    return true;
                }
                self.voice_enabled = !self.voice_enabled;
                if self.voice_enabled {
                    self.mult_choice = false;
                }
                true
            }
            SetupField::MultChoice => {
                self.mult_choice = !self.mult_choice;
                if self.mult_choice {
                    self.voice_enabled = false;
                }
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
            voice_enabled: self.voice_enabled,
            mult_choice: self.mult_choice,
            wrong_penalty: if self.penalize_wrong { -1 } else { 0 },
            large_text: self.large_text,
        })
    }
}

/// Takes the state by reference so a caller can re-enter the menu (e.g. after
/// voice startup fails) without discarding the ranges the user already typed.
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

            // Keep the two-column split but use only the left half; the menu
            // stacks in a single column while Start/Status still span the width.
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);

            render_menu_column(frame, columns[0], setup, &mut click_targets);

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

/// The menu, stacked top-to-bottom in a single column: Time, Operations,
/// Ranges, then Options.
fn render_menu_column(
    frame: &mut ratatui::Frame,
    area: Rect,
    setup: &SetupState,
    click_targets: &mut Vec<(Rect, SetupField)>,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Time
            Constraint::Length(8), // Operations
            Constraint::Length(5), // Ranges
            Constraint::Length(6), // Options
            Constraint::Min(0),
        ])
        .split(area);

    // --- Time. ---
    let time_block = Block::default()
        .title("Time (seconds)")
        .borders(Borders::ALL)
        .padding(Padding::left(1));
    let time_inner = time_block.inner(rows[0]);
    frame.render_widget(time_block, rows[0]);
    frame.render_widget(
        Paragraph::new(value_line(
            setup.time_input.as_str(),
            setup.focus == SetupField::TimeSeconds,
        )),
        time_inner,
    );
    click_targets.push((rows[0], SetupField::TimeSeconds));

    // --- Operations: 2x2 clickable grid, mirroring the answer grid. ---
    let ops_block = Block::default().title("Operations").borders(Borders::ALL);
    let ops_inner = ops_block.inner(rows[1]);
    frame.render_widget(ops_block, rows[1]);

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
    let ranges_inner = ranges_block.inner(rows[2]);
    frame.render_widget(ranges_block, rows[2]);

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

    // --- Options: one clickable toggle per row. ---
    let opts_block = Block::default()
        .title("Options")
        .borders(Borders::ALL)
        .padding(Padding::left(1));
    let opts_inner = opts_block.inner(rows[3]);
    frame.render_widget(opts_block, rows[3]);

    let opt_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(opts_inner);

    let option_lines = [
        (
            large_text_line(setup.large_text, setup.focus == SetupField::LargeText),
            SetupField::LargeText,
        ),
        (
            voice_line(setup.voice_enabled, setup.focus == SetupField::Voice),
            SetupField::Voice,
        ),
        (
            mult_choice_line(setup.mult_choice, setup.focus == SetupField::MultChoice),
            SetupField::MultChoice,
        ),
        (
            penalty_line(
                setup.penalize_wrong,
                setup.focus == SetupField::WrongPenalty,
                setup.mult_choice,
            ),
            SetupField::WrongPenalty,
        ),
    ];
    for (idx, (line, field)) in option_lines.into_iter().enumerate() {
        frame.render_widget(Paragraph::new(line), opt_rows[idx]);
        click_targets.push((opt_rows[idx], field));
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

fn voice_line(enabled: bool, focused: bool) -> Line<'static> {
    if !voice::AVAILABLE {
        let dim = Style::default().fg(Color::DarkGray);
        let label_style = if focused {
            dim.add_modifier(Modifier::BOLD)
        } else {
            dim
        };
        return Line::from(vec![
            Span::styled("Voice input:  ", label_style),
            Span::styled("[ ]  (--features voice)", dim),
        ]);
    }
    let label_style = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled("Voice input:  ", label_style),
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
    fn voice_flag_only_takes_effect_when_compiled_in() {
        let state = SetupState::new(true, false, true);
        assert_eq!(state.voice_enabled, voice::AVAILABLE);
        if !voice::AVAILABLE {
            assert_eq!(state.message, voice::UNSUPPORTED);
        }
    }

    #[test]
    fn toggling_voice_is_a_noop_without_support() {
        let mut state = SetupState::new(false, false, true);
        state.focus = SetupField::Voice;
        assert!(
            state.toggle_focused_mode(),
            "toggle should request a redraw"
        );
        assert_eq!(state.voice_enabled, voice::AVAILABLE);
    }

    #[test]
    fn voice_failure_disables_voice_and_reports_reason() {
        let mut state = SetupState::new(false, false, true);
        state.voice_enabled = true;
        state.voice_failed(String::from("no Vosk model found"));
        assert!(!state.voice_enabled);
        assert!(state.message.contains("no Vosk model found"));
    }

    #[cfg(not(feature = "voice"))]
    #[test]
    fn voice_row_shows_rebuild_hint() {
        use ratatui::backend::TestBackend;
        let mut terminal = Terminal::new(TestBackend::new(60, 1)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(Paragraph::new(vec![voice_line(false, false)]), frame.area())
            })
            .unwrap();
        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            rendered.contains("--features voice"),
            "{rendered}"
        );
    }

    #[test]
    fn mult_choice_flag_wins_over_voice() {
        let state = SetupState::new(true, true, true);
        assert!(!state.voice_enabled);
        assert!(state.mult_choice);
    }
}
