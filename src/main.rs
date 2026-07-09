mod game;
mod model;
mod results;
mod setup;
mod voice;

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
use ratatui::widgets::Paragraph;

use crate::game::run_game;
use crate::model::App;
use crate::results::{RecentAttempt, ResultsAction, run_results};
use crate::setup::{SetupState, run_setup};
use crate::voice::VoiceEngine;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    // -s presets the "Large text" setup toggle to off.
    let large_text_default = !args.iter().any(|arg| arg == "-s");
    let voice_default = args.iter().any(|arg| arg == "-v" || arg == "--voice");
    let mult_choice_default = args.iter().any(|arg| arg == "-m" || arg == "--mult-choice");
    let mut terminal = init_terminal()?;
    let result = run(&mut terminal, large_text_default, voice_default, mult_choice_default);
    restore_terminal(&mut terminal)?;

    match result {
        Ok(Some(_)) => Ok(()),
        Ok(None) => {
            println!("Canceled.");
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    large_text_default: bool,
    voice_default: bool,
    mult_choice_default: bool,
) -> Result<Option<App>, Box<dyn Error>> {
    let mut state = SetupState::new(voice_default, mult_choice_default, large_text_default);

    // Voice startup can fail (missing model, no microphone). Report it in the
    // menu and let the user start a keyboard drill instead of exiting.
    let (setup, voice) = loop {
        let setup = match run_setup(terminal, &mut state)? {
            Some(config) => config,
            None => return Ok(None),
        };
        if !setup.voice_enabled {
            break (setup, None);
        }
        terminal.draw(|frame| {
            frame.render_widget(Paragraph::new("Loading voice model..."), frame.area());
        })?;
        match VoiceEngine::start() {
            Ok(engine) => {
                // Native libs may have written to stderr during load; repaint.
                terminal.clear()?;
                break (setup, Some(engine));
            }
            Err(err) => {
                terminal.clear()?;
                state.voice_failed(err);
            }
        }
    };

    let mut game_config = setup.game;
    let mut duration = setup.duration;
    let mult_choice = setup.mult_choice;
    let wrong_penalty = setup.wrong_penalty;
    let use_small_text = !setup.large_text;
    let mut recent_attempts: Vec<RecentAttempt> = Vec::new();

    loop {
        let app = run_game(
            terminal,
            game_config.clone(),
            duration,
            use_small_text,
            voice.as_ref(),
            mult_choice,
            wrong_penalty,
        )?;
        recent_attempts.push(RecentAttempt {
            score: app.score,
        });
        if recent_attempts.len() > 10 {
            let overflow = recent_attempts.len() - 10;
            recent_attempts.drain(0..overflow);
        }

        match run_results(terminal, &app, &recent_attempts)? {
            ResultsAction::Restart => {
                game_config = app.config.clone();
                duration = app.duration;
            }
            ResultsAction::Exit => return Ok(Some(app)),
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
