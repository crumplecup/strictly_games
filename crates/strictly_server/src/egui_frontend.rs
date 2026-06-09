//! Egui native-window frontend for Strictly Games.
//!
//! Renders a game browser in a native OS window using the winit 0.30
//! `ApplicationHandler` pattern, `egui-winit` for event integration, and
//! `egui-wgpu` for GPU-accelerated rendering — the same stack as the archive
//! egui frontend.
//!
//! Key bindings:
//! - `↑`/`↓`/`←`/`→` or `W`/`A`/`S`/`D` — move cursor
//! - `Enter` — place piece / confirm
//! - `N` — new game
//! - `Q`/`Esc` — quit
//!
//! [`run_egui`] blocks on the OS main thread until the user closes the window.

#![cfg(not(kani))]

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use egui_winit::State as EguiWinitState;
use tracing::{error, info, instrument, warn};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

use strictly_tictactoe::{Board, Player, Position, TttDisplayMode};

use crate::games::tictactoe::AnyGame;
use crate::lobby::screen::{Screen, ScreenTransition};
use crate::lobby::screens::{
    AgentSelectScreen, BlackjackSetupScreen, GameSelectScreen, InGameScreen, MainLobbyScreen,
    ProfileSelectScreen, SettingsScreen, StatsViewScreen,
};
use crate::lobby::settings::{GameType, LobbySettings};
use crate::tui::GameEvent;
use crate::tui::contracts::TttUiConsistent;
use crate::tui::game_ir::{EventLog, GraphParams, ttt_to_verified_tree};
use crate::tui::{tictactoe_active, tictactoe_edges, tictactoe_nodes};
use crate::{AgentLibrary, ProfileService, User};

// ── Colour helpers ────────────────────────────────────────────────────────────

fn to_color32(c: elicit_ui::SrgbColor) -> egui::Color32 {
    egui::Color32::from_rgb(
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8,
    )
}

// ── Active screen state ───────────────────────────────────────────────────────

/// Current screen in the egui lobby / game state machine.
enum EguiActiveScreen {
    ProfileSelect(ProfileSelectScreen),
    MainLobby(MainLobbyScreen),
    GameSelect(GameSelectScreen),
    AgentSelect(AgentSelectScreen),
    BlackjackSetup(BlackjackSetupScreen),
    StatsView(StatsViewScreen),
    Settings(SettingsScreen),
    InGame(InGameScreen),
    /// Local TTT game — rendered through the game IR pipeline.
    TicTacToe {
        game: AnyGame,
        cursor: Position,
        events: Vec<GameEvent>,
    },
}

// ── Application struct ────────────────────────────────────────────────────────

/// Egui native-window frontend, driven by the WCAG AccessKit IR pipeline.
///
/// All screens — lobby navigation and in-game — are expressed as
/// `to_verified_tree() → EguiBackend::render()` calls so there is no
/// frontend-specific rendering logic.
struct GamesEguiApp {
    screen: EguiActiveScreen,
    should_quit: bool,
    profile_service: ProfileService,
    agent_library: AgentLibrary,
    current_user: Option<User>,
    settings: LobbySettings,

    // ── wgpu / egui-winit resources (None until `resumed`) ───────────────────
    window: Option<Arc<Window>>,
    egui_state: Option<EguiWinitState>,
    surface: Option<wgpu::Surface<'static>>,
    device: Option<Arc<wgpu::Device>>,
    queue: Option<Arc<wgpu::Queue>>,
    renderer: Option<egui_wgpu::Renderer>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
}

impl GamesEguiApp {
    fn new_with_lobby(profile_service: ProfileService, agent_library: AgentLibrary) -> Self {
        let screen = EguiActiveScreen::ProfileSelect(ProfileSelectScreen::new(&profile_service));
        Self {
            screen,
            should_quit: false,
            profile_service,
            agent_library,
            current_user: None,
            settings: LobbySettings::default(),
            window: None,
            egui_state: None,
            surface: None,
            device: None,
            queue: None,
            renderer: None,
            surface_config: None,
        }
    }

    /// Apply the Catppuccin Mocha colour palette to an egui context.
    fn apply_theme(ctx: &egui::Context) {
        use elicit_ui::{SemanticRole, palettes};
        let palette = palettes::mocha();
        let bg = to_color32(palette.color(SemanticRole::Background));
        let surface = to_color32(palette.color(SemanticRole::Surface));
        let text = to_color32(palette.color(SemanticRole::Text));
        let dim = to_color32(palette.color(SemanticRole::DimText));
        let accent = to_color32(palette.color(SemanticRole::Accent));
        let accent_c = palette.color(SemanticRole::Accent);
        let accent_alpha = egui::Color32::from_rgba_unmultiplied(
            (accent_c.r * 255.0).round() as u8,
            (accent_c.g * 255.0).round() as u8,
            (accent_c.b * 255.0).round() as u8,
            60,
        );
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = bg;
        visuals.window_fill = surface;
        visuals.extreme_bg_color = surface;
        visuals.code_bg_color = surface;
        visuals.override_text_color = Some(text);
        visuals.selection.bg_fill = accent_alpha;
        visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent);
        visuals.widgets.noninteractive.bg_fill = bg;
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, dim);
        visuals.widgets.inactive.bg_fill = surface;
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, text);
        visuals.widgets.hovered.bg_fill = surface;
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, text);
        visuals.widgets.active.bg_fill = surface;
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, accent);
        ctx.set_visuals(visuals);
    }

    // ── Keyboard input / Input handling ──────────────────────────────────────
    // Maps egui key events to game actions for the current frame.

    /// Process all egui events for this frame, applying at most one transition.
    #[instrument(skip(self, ctx))]
    fn handle_input(&mut self, ctx: &egui::Context) {
        let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());
        for ev in &events {
            let transition = self.transition_for_event(ev);
            if !matches!(transition, ScreenTransition::Stay) {
                self.apply_transition(transition);
                break;
            }
        }
    }

    /// Compute the `ScreenTransition` produced by a single egui event.
    fn transition_for_event(&mut self, ev: &egui::Event) -> ScreenTransition {
        let profile_service = &self.profile_service;
        match &mut self.screen {
            EguiActiveScreen::TicTacToe {
                game,
                cursor,
                events,
            } => ttt_handle_egui_key(game, cursor, events, ev),
            EguiActiveScreen::ProfileSelect(s) => egui_ev_to_key(ev)
                .map(|k| s.handle_key(k, profile_service))
                .unwrap_or(ScreenTransition::Stay),
            EguiActiveScreen::MainLobby(s) => egui_ev_to_key(ev)
                .map(|k| s.handle_key(k, profile_service))
                .unwrap_or(ScreenTransition::Stay),
            EguiActiveScreen::GameSelect(s) => egui_ev_to_key(ev)
                .map(|k| s.handle_key(k, profile_service))
                .unwrap_or(ScreenTransition::Stay),
            EguiActiveScreen::AgentSelect(s) => egui_ev_to_key(ev)
                .map(|k| s.handle_key(k, profile_service))
                .unwrap_or(ScreenTransition::Stay),
            EguiActiveScreen::BlackjackSetup(s) => egui_ev_to_key(ev)
                .map(|k| s.handle_key(k, profile_service))
                .unwrap_or(ScreenTransition::Stay),
            EguiActiveScreen::StatsView(s) => egui_ev_to_key(ev)
                .map(|k| s.handle_key(k, profile_service))
                .unwrap_or(ScreenTransition::Stay),
            EguiActiveScreen::Settings(s) => egui_ev_to_key(ev)
                .map(|k| s.handle_key(k, profile_service))
                .unwrap_or(ScreenTransition::Stay),
            EguiActiveScreen::InGame(s) => egui_ev_to_key(ev)
                .map(|k| s.handle_key(k, profile_service))
                .unwrap_or(ScreenTransition::Stay),
        }
    }

    /// Apply a `ScreenTransition`, updating `self.screen` and related state.
    fn apply_transition(&mut self, transition: ScreenTransition) {
        match transition {
            ScreenTransition::Stay => {}

            ScreenTransition::Quit => {
                self.should_quit = true;
            }

            ScreenTransition::GoToProfileSelect => {
                self.screen = EguiActiveScreen::ProfileSelect(ProfileSelectScreen::new(
                    &self.profile_service,
                ));
            }

            ScreenTransition::GoToMainLobby => {
                // Extract settings if leaving the settings screen.
                if let EguiActiveScreen::Settings(s) = &self.screen {
                    self.settings = s.settings();
                }
                // Extract the newly selected user if leaving profile select.
                if let EguiActiveScreen::ProfileSelect(s) = &self.screen
                    && let Some(user_id) = s.selected_user_id().as_deref()
                {
                    let handle = tokio::runtime::Handle::current();
                    if let Ok(Some(user)) = tokio::task::block_in_place(|| {
                        handle.block_on(self.profile_service.repository().get_user_by_id(user_id))
                    }) {
                        self.current_user = Some(user);
                    }
                }
                if let Some(user) = &self.current_user {
                    self.screen = EguiActiveScreen::MainLobby(MainLobbyScreen::with_game(
                        user.clone(),
                        self.settings.selected_game,
                        &self.profile_service,
                    ));
                } else {
                    self.screen = EguiActiveScreen::ProfileSelect(ProfileSelectScreen::new(
                        &self.profile_service,
                    ));
                }
            }

            ScreenTransition::GoToGameSelect => {
                self.screen = EguiActiveScreen::GameSelect(GameSelectScreen::new(
                    self.settings.selected_game,
                ));
            }

            ScreenTransition::GameSelected { game } => {
                self.settings.selected_game = game;
                self.apply_transition(ScreenTransition::GoToMainLobby);
            }

            ScreenTransition::GoToAgentSelect => {
                self.screen =
                    EguiActiveScreen::AgentSelect(AgentSelectScreen::new(&self.agent_library));
            }

            ScreenTransition::GoToStatsView => {
                if let Some(user) = &self.current_user {
                    self.screen = EguiActiveScreen::StatsView(StatsViewScreen::new(
                        user.clone(),
                        &self.profile_service,
                    ));
                }
            }

            ScreenTransition::GoToSettings => {
                self.screen = EguiActiveScreen::Settings(SettingsScreen::new(self.settings));
            }

            ScreenTransition::GoToInGame { agent_name } => {
                if self.settings.selected_game == GameType::TicTacToe {
                    self.screen = EguiActiveScreen::TicTacToe {
                        game: AnyGame::InProgress {
                            board: Board::default(),
                            to_move: Player::X,
                            history: Vec::new(),
                        },
                        cursor: Position::Center,
                        events: vec![GameEvent::story(format!("🎮 Game begins vs {agent_name}"))],
                    };
                } else {
                    self.screen = EguiActiveScreen::InGame(InGameScreen::new(agent_name));
                }
            }

            ScreenTransition::GoToBlackjackSetup => {
                let name = self
                    .current_user
                    .as_ref()
                    .map(|u| u.display_name().clone())
                    .unwrap_or_else(|| "Player".to_string());
                self.screen = EguiActiveScreen::BlackjackSetup(BlackjackSetupScreen::new(
                    name,
                    &self.agent_library,
                ));
            }

            ScreenTransition::GoToBlackjackTable { .. } => {
                // Async multi-player session not yet implemented in egui frontend.
                self.screen =
                    EguiActiveScreen::InGame(InGameScreen::new("Blackjack Table".to_string()));
            }
        }
    }

    // ── Per-frame render ──────────────────────────────────────────────────────

    /// Called once per frame: process input then render the current screen.
    ///
    /// All screens are expressed as `to_verified_tree() → EguiBackend::render()`
    /// so no frontend-specific widget code lives here.
    #[instrument(skip(self, ui))]
    fn render_ui(&mut self, ui: &mut egui::Ui) {
        use elicit_egui::EguiBackend;
        use elicit_ui::{UiTreeRenderer as _, Viewport};

        let ctx = ui.ctx().clone();
        self.handle_input(&ctx);

        let size = ui.available_size();
        let viewport = Viewport::new(size.x as u32, size.y as u32);

        match &self.screen {
            EguiActiveScreen::TicTacToe {
                game,
                cursor,
                events,
            } => {
                let _proof = render_ttt_egui(ui, game, cursor, events);
            }
            screen => {
                let tree = match screen {
                    EguiActiveScreen::ProfileSelect(s) => s.to_verified_tree(viewport),
                    EguiActiveScreen::MainLobby(s) => s.to_verified_tree(viewport),
                    EguiActiveScreen::GameSelect(s) => s.to_verified_tree(viewport),
                    EguiActiveScreen::AgentSelect(s) => s.to_verified_tree(viewport),
                    EguiActiveScreen::BlackjackSetup(s) => s.to_verified_tree(viewport),
                    EguiActiveScreen::StatsView(s) => s.to_verified_tree(viewport),
                    EguiActiveScreen::Settings(s) => s.to_verified_tree(viewport),
                    EguiActiveScreen::InGame(s) => s.to_verified_tree(viewport),
                    EguiActiveScreen::TicTacToe { .. } => {
                        tracing::error!("TicTacToe screen reached egui render path — not implemented");
                        return;
                    }
                };
                let backend = EguiBackend::new();
                match backend.render(&tree) {
                    Ok((widget, _stats, _proof)) => widget(ui),
                    Err(e) => {
                        error!(error = %e, "EguiBackend render failed");
                        ui.label(format!("Render error: {e}"));
                    }
                }
            }
        }
    }
}

// ── Input helpers ──────────────────────────────────────────────────────────────

/// Convert an egui key event to a crossterm [`KeyEvent`] for dispatch to lobby screens.
///
/// Returns `None` for events that don't correspond to a lobby action
/// (mouse, scroll, IME, etc.).
fn egui_ev_to_key(ev: &egui::Event) -> Option<KeyEvent> {
    let (key, modifiers) = match ev {
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => (*key, *modifiers),
        _ => return None,
    };
    use egui::Key::*;
    let code = match key {
        ArrowUp => KeyCode::Up,
        ArrowDown => KeyCode::Down,
        ArrowLeft => KeyCode::Left,
        ArrowRight => KeyCode::Right,
        Enter => KeyCode::Enter,
        Escape => KeyCode::Esc,
        Backspace => KeyCode::Backspace,
        Space => KeyCode::Char(' '),
        A => KeyCode::Char(if modifiers.shift { 'A' } else { 'a' }),
        B => KeyCode::Char(if modifiers.shift { 'B' } else { 'b' }),
        N => KeyCode::Char(if modifiers.shift { 'N' } else { 'n' }),
        Q => KeyCode::Char(if modifiers.shift { 'Q' } else { 'q' }),
        S => KeyCode::Char(if modifiers.shift { 'S' } else { 's' }),
        T => KeyCode::Char(if modifiers.shift { 'T' } else { 't' }),
        _ => return None,
    };
    Some(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

/// Handle an egui event while the TTT game is active.
///
/// Converts movement / action keys to game moves and returns a
/// [`ScreenTransition`] for Escape / Quit.
fn ttt_handle_egui_key(
    game: &mut AnyGame,
    cursor: &mut Position,
    events: &mut Vec<GameEvent>,
    ev: &egui::Event,
) -> ScreenTransition {
    let egui::Event::Key {
        key, pressed: true, ..
    } = ev
    else {
        return ScreenTransition::Stay;
    };
    use egui::Key::*;
    match key {
        ArrowUp | K | W => {
            *cursor = cursor_up(*cursor);
        }
        ArrowDown | J => {
            *cursor = cursor_down(*cursor);
        }
        ArrowLeft | H | A => {
            *cursor = cursor_left(*cursor);
        }
        ArrowRight | L | D => {
            *cursor = cursor_right(*cursor);
        }
        Enter | Space => {
            let pos = *cursor;
            let cur_game = std::mem::replace(
                game,
                AnyGame::Setup {
                    board: Board::default(),
                },
            );
            let mover = cur_game.to_move();
            match cur_game.make_move_action(strictly_tictactoe::action::Move::new(
                mover.unwrap_or(Player::X),
                pos,
            )) {
                Ok(next) => {
                    let player = if mover == Some(Player::X) { "X" } else { "O" };
                    events.push(GameEvent::story(format!(
                        "  {} {player} plays {pos}",
                        if player == "X" { "✕" } else { "◯" },
                        pos = pos.label(),
                    )));
                    if next.is_over() {
                        if let Some(winner) = next.winner() {
                            events.push(GameEvent::result(format!("🏆 {winner:?} wins!")));
                        } else {
                            events.push(GameEvent::result("🤝 Draw — the board is full"));
                        }
                    }
                    *game = next;
                }
                Err(e) => {
                    warn!(error = %e, "Illegal move");
                    events.push(GameEvent::story(format!("⚠ {e}")));
                    *game = AnyGame::InProgress {
                        board: Board::default(),
                        to_move: Player::X,
                        history: Vec::new(),
                    };
                }
            }
        }
        N => {
            *game = AnyGame::InProgress {
                board: Board::default(),
                to_move: Player::X,
                history: Vec::new(),
            };
            *cursor = Position::Center;
            events.clear();
            events.push(GameEvent::story("🎮 New game — X moves first"));
        }
        Escape => return ScreenTransition::GoToMainLobby,
        Q => return ScreenTransition::Quit,
        _ => {}
    }
    ScreenTransition::Stay
}

// ── IR render helpers ──────────────────────────────────────────────────────────

/// Gate function: renders TTT state through the WCAG IR pipeline into `ui`.
///
/// Returns `Established<TttUiConsistent>` — the game-level proof that a
/// complete IR-to-egui render occurred.
#[instrument(skip(ui, game, events))]
fn render_ttt_egui(
    ui: &mut egui::Ui,
    game: &AnyGame,
    cursor: &Position,
    events: &[GameEvent],
) -> elicitation::contracts::Established<TttUiConsistent> {
    use elicit_ui::{UiTreeRenderer as _, Viewport};
    use elicitation::contracts::Established;

    let size = ui.available_size();
    let vp = Viewport::new(size.x as u32, size.y as u32);

    let ttt_nodes = tictactoe_nodes();
    let ttt_edges = tictactoe_edges();
    let log = EventLog {
        events,
        dialogue: &[],
    };
    let graph = GraphParams {
        nodes: &ttt_nodes,
        edges: &ttt_edges,
        active: tictactoe_active(game),
    };

    let tree = ttt_to_verified_tree(
        game,
        &TttDisplayMode::BoardWithCursor(*cursor),
        &log,
        &graph,
        vp,
    );

    // Proof chain: VerifiedTree → WcagVerified (inside UiTreeRenderer) →
    // RenderComplete → TttUiConsistent.
    use elicit_egui::EguiBackend;
    let backend = EguiBackend::new();
    match backend.render(&tree) {
        Ok((widget, _stats, render_proof)) => {
            widget(ui);
            Established::prove(&render_proof)
        }
        Err(e) => {
            error!(error = %e, "EguiBackend::render failed");
            ui.label(format!("Render error: {e}"));
            Established::assert()
        }
    }
}

// ── Cursor helpers ────────────────────────────────────────────────────────────

fn cursor_up(pos: Position) -> Position {
    match pos {
        Position::TopLeft => Position::TopLeft,
        Position::TopCenter => Position::TopCenter,
        Position::TopRight => Position::TopRight,
        Position::MiddleLeft => Position::TopLeft,
        Position::Center => Position::TopCenter,
        Position::MiddleRight => Position::TopRight,
        Position::BottomLeft => Position::MiddleLeft,
        Position::BottomCenter => Position::Center,
        Position::BottomRight => Position::MiddleRight,
    }
}

fn cursor_down(pos: Position) -> Position {
    match pos {
        Position::TopLeft => Position::MiddleLeft,
        Position::TopCenter => Position::Center,
        Position::TopRight => Position::MiddleRight,
        Position::MiddleLeft => Position::BottomLeft,
        Position::Center => Position::BottomCenter,
        Position::MiddleRight => Position::BottomRight,
        Position::BottomLeft => Position::BottomLeft,
        Position::BottomCenter => Position::BottomCenter,
        Position::BottomRight => Position::BottomRight,
    }
}

fn cursor_left(pos: Position) -> Position {
    match pos {
        Position::TopLeft => Position::TopLeft,
        Position::TopCenter => Position::TopLeft,
        Position::TopRight => Position::TopCenter,
        Position::MiddleLeft => Position::MiddleLeft,
        Position::Center => Position::MiddleLeft,
        Position::MiddleRight => Position::Center,
        Position::BottomLeft => Position::BottomLeft,
        Position::BottomCenter => Position::BottomLeft,
        Position::BottomRight => Position::BottomCenter,
    }
}

fn cursor_right(pos: Position) -> Position {
    match pos {
        Position::TopLeft => Position::TopCenter,
        Position::TopCenter => Position::TopRight,
        Position::TopRight => Position::TopRight,
        Position::MiddleLeft => Position::Center,
        Position::Center => Position::MiddleRight,
        Position::MiddleRight => Position::MiddleRight,
        Position::BottomLeft => Position::BottomCenter,
        Position::BottomCenter => Position::BottomRight,
        Position::BottomRight => Position::BottomRight,
    }
}

// ── winit ApplicationHandler ──────────────────────────────────────────────────

impl ApplicationHandler for GamesEguiApp {
    #[instrument(skip(self, event_loop))]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("Strictly Games")
            .with_inner_size(winit::dpi::LogicalSize::new(1280_f64, 720_f64));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                tracing::error!(error = %e, "failed to create window — egui frontend unavailable");
                return;
            }
        };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = match instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "failed to create wgpu surface — egui frontend unavailable");
                return;
            }
        };

        // wgpu async init — safe to block here: running on a plain std thread.
        let adapter = match futures::executor::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            },
        )) {
            Ok(a) => a,
            Err(e) => {
                tracing::error!(error = %e, "no suitable wgpu adapter — egui frontend unavailable");
                return;
            }
        };
        let (device, queue) =
            match futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(error = %e, "wgpu device creation failed — egui frontend unavailable");
                    return;
                }
            };
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let egui_ctx = egui::Context::default();
        Self::apply_theme(&egui_ctx);
        let egui_state = EguiWinitState::new(
            egui_ctx,
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let renderer =
            egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());

        self.window = Some(window);
        self.egui_state = Some(egui_state);
        self.surface = Some(surface);
        self.device = Some(device);
        self.queue = Some(queue);
        self.renderer = Some(renderer);
        self.surface_config = Some(config);

        info!("egui window created");
    }

    #[instrument(skip(self, event_loop))]
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let window = match self.window.as_ref() {
            Some(w) => w.clone(),
            None => return,
        };
        let state = match self.egui_state.as_mut() {
            Some(s) => s,
            None => return,
        };

        let response = state.on_window_event(&window, &event);
        if response.repaint {
            window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let (Some(surface), Some(device), Some(cfg)) = (
                    self.surface.as_ref(),
                    self.device.as_ref(),
                    self.surface_config.as_mut(),
                ) {
                    cfg.width = size.width.max(1);
                    cfg.height = size.height.max(1);
                    surface.configure(device, cfg);
                }
                window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                let Some(egui_state) = self.egui_state.as_mut() else {
                    tracing::warn!("RedrawRequested before egui state was initialised — skipping");
                    return;
                };
                let raw = egui_state.take_egui_input(&window);
                let ctx = egui_state.egui_ctx().clone();

                let out = ctx.run_ui(raw, |ui| self.render_ui(ui));

                if self.should_quit {
                    event_loop.exit();
                    return;
                }

                let Some(egui_state) = self.egui_state.as_mut() else {
                    return;
                };
                egui_state.handle_platform_output(&window, out.platform_output);

                let (surface, device, queue, renderer, cfg) = match (
                    self.surface.as_ref(),
                    self.device.as_ref(),
                    self.queue.as_ref(),
                    self.renderer.as_mut(),
                    self.surface_config.as_ref(),
                ) {
                    (Some(s), Some(d), Some(q), Some(r), Some(c)) => (s, d, q, r, c),
                    _ => return,
                };

                let surface_tex = surface.get_current_texture();
                let texture = match surface_tex {
                    wgpu::CurrentSurfaceTexture::Success(t) => t,
                    _ => return,
                };
                let view = texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let clipped = ctx.tessellate(out.shapes, ctx.pixels_per_point());
                let screen = egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [cfg.width, cfg.height],
                    pixels_per_point: ctx.pixels_per_point(),
                };
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                for (id, delta) in &out.textures_delta.set {
                    renderer.update_texture(device, queue, *id, delta);
                }
                renderer.update_buffers(device, queue, &mut encoder, &clipped, &screen);
                {
                    let mut rpass = encoder
                        .begin_render_pass(&wgpu::RenderPassDescriptor {
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                resolve_target: None,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color {
                                        r: 30.0 / 255.0,
                                        g: 30.0 / 255.0,
                                        b: 46.0 / 255.0,
                                        a: 1.0,
                                    }),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            ..Default::default()
                        })
                        .forget_lifetime();
                    renderer.render(&mut rpass, &clipped, &screen);
                }
                queue.submit(std::iter::once(encoder.finish()));
                texture.present();
                for id in &out.textures_delta.free {
                    renderer.free_texture(id);
                }
                window.request_redraw();
            }

            _ => {}
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Run the egui native-window frontend, blocking until the user closes it.
///
/// **Must be called from the OS main thread.**
///
/// # Errors
///
/// Returns an error if the winit event loop fails to start.
#[instrument(skip(profile_service, agent_library))]
pub fn run_egui(
    profile_service: ProfileService,
    agent_library: AgentLibrary,
) -> anyhow::Result<()> {
    info!("Starting egui frontend");
    let event_loop = EventLoop::new()?;
    let mut app = GamesEguiApp::new_with_lobby(profile_service, agent_library);
    event_loop.run_app(&mut app)?;
    Ok(())
}
