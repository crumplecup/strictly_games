//! Lobby controller — the state machine driving the multi-screen TUI.

use std::path::PathBuf;

use crossterm::event::{self, Event, KeyEventKind};
use derive_getters::Getters;
use ratatui::{Terminal, backend::Backend};
use tokio::time::{Duration, sleep};
use tracing::{debug, info, instrument, warn};

use crate::lobby::screen::ScreenTransition;
use crate::lobby::screens::{
    AgentSelectScreen, InGameScreen, MainLobbyScreen, ProfileSelectScreen, SettingsScreen,
    StatsViewScreen,
};
use crate::lobby::settings::LobbySettings;
use crate::run_game_session;
use crate::{
    AgentConfig, AgentLibrary, AnyGame, FirstPlayer, GameOutcome, ProfileService, TicTacToePlayer,
    User,
};

/// Active screen in the lobby state machine.
#[derive(Debug)]
enum ActiveScreen {
    ProfileSelect(ProfileSelectScreen),
    MainLobby(MainLobbyScreen),
    AgentSelect(AgentSelectScreen),
    StatsView(StatsViewScreen),
    InGame(InGameScreen),
    Settings(SettingsScreen),
}

/// Controller that drives the lobby state machine.
///
/// Call [`LobbyController::run`] to start the event loop.
#[derive(Debug, Getters)]
pub struct LobbyController {
    profile_service: ProfileService,
    agent_library: AgentLibrary,
    current_user: Option<User>,
    agent_config_path: PathBuf,
    server_port: u16,
    settings: LobbySettings,
}

impl LobbyController {
    /// Creates a new lobby controller.
    #[instrument(skip(profile_service, agent_library, agent_config_path))]
    pub fn new(
        profile_service: ProfileService,
        agent_library: AgentLibrary,
        agent_config_path: PathBuf,
        server_port: u16,
    ) -> Self {
        info!("Creating LobbyController");
        Self {
            profile_service,
            agent_library,
            current_user: None,
            agent_config_path,
            server_port,
            settings: LobbySettings::new(),
        }
    }

    /// Runs the lobby event loop until the user quits.
    ///
    /// Sets up the terminal, drives screen transitions, and restores the
    /// terminal on exit.
    #[instrument(skip(self, terminal))]
    pub async fn run<B: Backend + std::io::Write>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> anyhow::Result<()>
    where
        <B as Backend>::Error: Send + Sync + 'static,
    {
        info!("Starting lobby event loop");

        let mut screen =
            ActiveScreen::ProfileSelect(ProfileSelectScreen::new(&self.profile_service));

        loop {
            // Render current screen.
            terminal.draw(|f| {
                use crate::lobby::screen::Screen;
                match &screen {
                    ActiveScreen::ProfileSelect(s) => s.render(f, &self.profile_service),
                    ActiveScreen::MainLobby(s) => s.render(f, &self.profile_service),
                    ActiveScreen::AgentSelect(s) => s.render(f, &self.profile_service),
                    ActiveScreen::StatsView(s) => s.render(f, &self.profile_service),
                    ActiveScreen::InGame(s) => s.render(f, &self.profile_service),
                    ActiveScreen::Settings(s) => s.render(f, &self.profile_service),
                }
            })?;

            // Poll for input with short timeout to keep the loop responsive.
            if event::poll(Duration::from_millis(100))?
                && let Event::Key(key) = event::read()?
            {
                // Skip key release events (crossterm fires both press and release).
                if key.kind == KeyEventKind::Release {
                    continue;
                }

                use crate::lobby::screen::Screen;
                let transition = match &mut screen {
                    ActiveScreen::ProfileSelect(s) => s.handle_key(key, &self.profile_service),
                    ActiveScreen::MainLobby(s) => s.handle_key(key, &self.profile_service),
                    ActiveScreen::AgentSelect(s) => s.handle_key(key, &self.profile_service),
                    ActiveScreen::StatsView(s) => s.handle_key(key, &self.profile_service),
                    ActiveScreen::InGame(s) => s.handle_key(key, &self.profile_service),
                    ActiveScreen::Settings(s) => s.handle_key(key, &self.profile_service),
                };

                // GoToInGame runs the actual game loop before any other transition.
                if let ScreenTransition::GoToInGame { ref agent_name } = transition {
                    let agent_name = agent_name.clone();
                    match self
                        .execute_game(terminal, &agent_name, self.settings.first_player)
                        .await
                    {
                        Ok(next_screen) => {
                            screen = next_screen;
                            continue;
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Game session failed");
                            screen = match &self.current_user {
                                Some(user) => {
                                    ActiveScreen::MainLobby(MainLobbyScreen::new(user.clone()))
                                }
                                None => ActiveScreen::ProfileSelect(ProfileSelectScreen::new(
                                    &self.profile_service,
                                )),
                            };
                            continue;
                        }
                    }
                }

                screen = match self.apply_transition(transition, screen) {
                    Some(next) => next,
                    None => {
                        info!("Lobby quitting");
                        return Ok(());
                    }
                };
            }

            sleep(Duration::from_millis(10)).await;
        }
    }

    /// Applies a screen transition, returning the next screen or `None` to quit.
    #[instrument(skip(self, current))]
    fn apply_transition(
        &mut self,
        transition: ScreenTransition,
        current: ActiveScreen,
    ) -> Option<ActiveScreen> {
        debug!(transition = ?transition, "Applying screen transition");
        match transition {
            ScreenTransition::Stay => Some(current),

            ScreenTransition::GoToProfileSelect => {
                info!("Navigating to ProfileSelect");
                Some(ActiveScreen::ProfileSelect(ProfileSelectScreen::new(
                    &self.profile_service,
                )))
            }

            ScreenTransition::GoToMainLobby => {
                // Persist any settings changes if returning from the Settings screen.
                if let Some(updated) = self.extract_settings_from_screen(&current) {
                    debug!(
                        first_player = %updated.first_player.label(),
                        "Saving updated settings"
                    );
                    self.settings = updated;
                }

                let user = match self.extract_user_from_screen(&current) {
                    Some(u) => {
                        self.current_user = Some(u.clone());
                        u
                    }
                    None => match &self.current_user {
                        Some(u) => u.clone(),
                        None => {
                            warn!("No user available for MainLobby — redirecting to ProfileSelect");
                            return Some(ActiveScreen::ProfileSelect(ProfileSelectScreen::new(
                                &self.profile_service,
                            )));
                        }
                    },
                };
                info!(user_id = user.id(), "Navigating to MainLobby");
                Some(ActiveScreen::MainLobby(MainLobbyScreen::new(user)))
            }

            ScreenTransition::GoToAgentSelect => {
                info!("Navigating to AgentSelect");
                Some(ActiveScreen::AgentSelect(AgentSelectScreen::new(
                    &self.agent_library,
                )))
            }

            ScreenTransition::GoToStatsView => {
                let user = match &self.current_user {
                    Some(u) => u.clone(),
                    None => {
                        warn!("No user for StatsView — redirecting to ProfileSelect");
                        return Some(ActiveScreen::ProfileSelect(ProfileSelectScreen::new(
                            &self.profile_service,
                        )));
                    }
                };
                info!(user_id = user.id(), "Navigating to StatsView");
                Some(ActiveScreen::StatsView(StatsViewScreen::new(
                    user,
                    &self.profile_service,
                )))
            }

            ScreenTransition::GoToSettings => {
                info!("Navigating to Settings");
                Some(ActiveScreen::Settings(SettingsScreen::new(self.settings)))
            }

            ScreenTransition::GoToInGame { agent_name } => {
                info!(agent_name = %agent_name, "Navigating to InGame");
                Some(ActiveScreen::InGame(InGameScreen::new(agent_name)))
            }

            ScreenTransition::Quit => None,
        }
    }

    /// Extracts the selected user from screens that perform profile selection.
    #[instrument(skip(self, screen))]
    fn extract_user_from_screen(&self, screen: &ActiveScreen) -> Option<User> {
        match screen {
            ActiveScreen::ProfileSelect(s) => {
                let user_id = (*s.selected_user_id())?;
                self.profile_service
                    .repository()
                    .get_user_by_name(
                        s.users()
                            .iter()
                            .find(|u| *u.id() == user_id)?
                            .display_name(),
                    )
                    .ok()
                    .flatten()
            }
            _ => None,
        }
    }

    /// Extracts updated settings from the settings screen when navigating away.
    #[instrument(skip(self, screen))]
    fn extract_settings_from_screen(&self, screen: &ActiveScreen) -> Option<LobbySettings> {
        match screen {
            ActiveScreen::Settings(s) => Some(s.settings()),
            _ => None,
        }
    }

    /// Finds an agent config by name in the library.
    #[instrument(skip(self))]
    pub fn find_agent(&self, name: &str) -> Option<&AgentConfig> {
        debug!(name = %name, "Looking up agent config");
        self.agent_library.get_by_name(name)
    }

    /// Runs a full game session against the named agent and returns the next screen.
    ///
    /// Looks up the agent config, spawns server + agent, runs the game loop, records
    /// the result, and returns an [`ActiveScreen::MainLobby`] for the transition back.
    #[instrument(skip(self, terminal))]
    async fn execute_game<B: Backend + std::io::Write>(
        &mut self,
        terminal: &mut Terminal<B>,
        agent_name: &str,
        first_player: FirstPlayer,
    ) -> anyhow::Result<ActiveScreen>
    where
        <B as Backend>::Error: Send + Sync + 'static,
    {
        info!(agent_name = %agent_name, first_player = %first_player.label(), "Executing game session");

        let agent_config = match self.agent_library.get_by_name(agent_name) {
            Some(c) => c.clone(),
            None => {
                warn!(agent_name = %agent_name, "Agent not found in library");
                let screen = match &self.current_user {
                    Some(u) => ActiveScreen::MainLobby(MainLobbyScreen::new(u.clone())),
                    None => {
                        ActiveScreen::ProfileSelect(ProfileSelectScreen::new(&self.profile_service))
                    }
                };
                return Ok(screen);
            }
        };

        let config_path = agent_config
            .config_path()
            .clone()
            .unwrap_or_else(|| self.agent_config_path.clone());

        let player_name = self
            .current_user
            .as_ref()
            .map(|u| u.display_name().clone())
            .unwrap_or_else(|| "Human".to_string());

        info!(
            config_path = %config_path.display(),
            player_name = %player_name,
            port = self.server_port,
            "Launching game session"
        );

        let (final_game, human_mark) = run_game_session(
            terminal,
            config_path,
            player_name.clone(),
            *self.server_port(),
            first_player,
        )
        .await?;

        // Record the result if there is a logged-in user.
        if let Some(user) = &self.current_user {
            let outcome = determine_outcome(&final_game, human_mark);
            let moves_count = final_game.history().len() as i32;
            debug!(
                user_id = user.id(),
                outcome = ?outcome,
                moves = moves_count,
                "Recording game result"
            );
            if let Err(e) = self.profile_service.record_game_result(
                *user.id(),
                agent_name.to_string(),
                "tictactoe".to_string(),
                outcome,
                moves_count,
                "tui_session".to_string(),
            ) {
                tracing::warn!(error = %e, "Failed to record game result");
            }
        }

        // Return to main lobby.
        let screen = match &self.current_user {
            Some(u) => ActiveScreen::MainLobby(MainLobbyScreen::new(u.clone())),
            None => ActiveScreen::ProfileSelect(ProfileSelectScreen::new(&self.profile_service)),
        };
        Ok(screen)
    }
}

/// Determines the game outcome from the human player's perspective.
#[instrument]
fn determine_outcome(game: &AnyGame, human_mark: TicTacToePlayer) -> GameOutcome {
    if let Some(winner) = game.winner() {
        if winner == human_mark {
            GameOutcome::Win
        } else {
            GameOutcome::Loss
        }
    } else {
        GameOutcome::Draw
    }
}
