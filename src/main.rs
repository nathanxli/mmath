mod game;
mod model;
mod optiver;
mod results;
mod sequences;
mod setup;

use std::env;
use std::error::Error;
use std::io;

use crossterm::event::DisableMouseCapture;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::game::run_game;
use crate::results::{ResultsAction, run_results};
use crate::setup::{SetupState, run_setup};

/// Scores kept for the results page's session statistics.
const MAX_RECENT_SCORES: usize = 10;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    // -s presets the "Large text" setup toggle to off.
    let large_text_default = !args.iter().any(|arg| arg == "-s");
    let mult_choice_default = args.iter().any(|arg| arg == "-m" || arg == "--mult-choice");
    let mut terminal = init_terminal()?;
    let result = run(&mut terminal, large_text_default, mult_choice_default);
    restore_terminal(&mut terminal)?;

    match result {
        Ok(true) => Ok(()),
        Ok(false) => {
            println!("Canceled.");
            Ok(())
        }
        Err(err) => Err(err),
    }
}

/// Returns false if the user canceled at the setup menu.
fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    large_text_default: bool,
    mult_choice_default: bool,
) -> Result<bool, Box<dyn Error>> {
    let mut state = SetupState::new(mult_choice_default, large_text_default);

    let setup = match run_setup(terminal, &mut state)? {
        Some(config) => config,
        None => return Ok(false),
    };

    let game_config = setup.game;
    let duration = setup.duration;
    let mult_choice = setup.mult_choice;
    let wrong_penalty = setup.wrong_penalty;
    let use_small_text = !setup.large_text;
    let mut recent_scores: Vec<i32> = Vec::new();

    loop {
        let app = run_game(
            terminal,
            game_config.clone(),
            duration,
            use_small_text,
            mult_choice,
            wrong_penalty,
        )?;
        recent_scores.push(app.score);
        if recent_scores.len() > MAX_RECENT_SCORES {
            recent_scores.remove(0);
        }

        match run_results(terminal, &app, &recent_scores)? {
            // Restart reuses the same config and duration, so there is nothing
            // to carry over -- just play another round.
            ResultsAction::Restart => {}
            ResultsAction::Exit => return Ok(true),
        }
    }
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
    // Mouse capture is only enabled during multiple-choice games, but
    // disabling it unconditionally is harmless and covers error exits.
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
