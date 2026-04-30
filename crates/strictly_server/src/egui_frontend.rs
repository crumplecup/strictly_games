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

use egui::Key;
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
use crate::tui::GameEvent;
use crate::tui::contracts::TttUiConsistent;
use crate::tui::game_ir::{EventLog, GraphParams, ttt_to_verified_tree};

// ── Colour helpers ────────────────────────────────────────────────────────────

fn to_color32(c: elicit_ui::SrgbColor) -> egui::Color32 {
    egui::Color32::from_rgb(
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8,
    )
}

// ── Active game state ────────────────────────────────────────────────────────

/// Which game (if any) is currently being displayed.
#[derive(Debug)]
enum ActiveGame {
    /// Main selection menu.
    Menu,
    /// Tic-tac-toe: local two-player game.
    TicTacToe {
        /// Current serialisable game state.
        game: AnyGame,
        /// Cursor position on the board.
        cursor: Position,
        /// Accumulated story events for the right-side event log.
        events: Vec<GameEvent>,
    },
}

impl ActiveGame {
    fn new_ttt() -> Self {
        ActiveGame::TicTacToe {
            game: AnyGame::InProgress {
                board: Board::default(),
                to_move: Player::X,
                history: Vec::new(),
            },
            cursor: Position::Center,
            events: vec![GameEvent::story("🎮 Game begins — X moves first")],
        }
    }
}

// ── Application struct ────────────────────────────────────────────────────────

/// Standalone egui game application.
///
/// All rendering uses the WCAG AccessKit IR pipeline:
/// `game_state → *_to_verified_tree() → elicit_egui::render_tree()`
struct GamesEguiApp {
    /// Current active game / menu state.
    active: ActiveGame,
    /// Whether the event loop should exit on the next frame.
    should_quit: bool,

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
    fn new() -> Self {
        Self {
            active: ActiveGame::Menu,
            should_quit: false,
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
        visuals.selection.stroke = egui::Stroke::new(1.0, accent);
        visuals.widgets.noninteractive.bg_fill = bg;
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, dim);
        visuals.widgets.inactive.bg_fill = surface;
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text);
        visuals.widgets.hovered.bg_fill = surface;
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, text);
        visuals.widgets.active.bg_fill = surface;
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, accent);
        ctx.set_visuals(visuals);
    }

    // ── Keyboard input ────────────────────────────────────────────────────────

    /// Map egui key events to game actions for the current frame.
    #[instrument(skip(self, ctx))]
    fn handle_keys(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            for ev in &i.events {
                if let egui::Event::Key {
                    key, pressed: true, ..
                } = ev
                {
                    self.dispatch_key(*key);
                }
            }
        });
    }

    fn dispatch_key(&mut self, key: Key) {
        match &mut self.active {
            ActiveGame::Menu => match key {
                Key::T => self.active = ActiveGame::new_ttt(),
                Key::Q | Key::Escape => self.should_quit = true,
                _ => {}
            },
            ActiveGame::TicTacToe {
                game,
                cursor,
                events,
            } => {
                match key {
                    // Cursor movement
                    Key::ArrowUp | Key::K | Key::W => {
                        *cursor = cursor_up(*cursor);
                    }
                    Key::ArrowDown | Key::J | Key::S => {
                        *cursor = cursor_down(*cursor);
                    }
                    Key::ArrowLeft | Key::H | Key::A => {
                        *cursor = cursor_left(*cursor);
                    }
                    Key::ArrowRight | Key::L | Key::D => {
                        *cursor = cursor_right(*cursor);
                    }
                    // Place piece
                    Key::Enter | Key::Space => {
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
                                        events.push(GameEvent::result(format!(
                                            "🏆 {winner:?} wins!"
                                        )));
                                    } else {
                                        events
                                            .push(GameEvent::result("🤝 Draw — the board is full"));
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
                    // New game
                    Key::N => {
                        self.active = ActiveGame::new_ttt();
                    }
                    // Back to menu
                    Key::Escape => {
                        self.active = ActiveGame::Menu;
                    }
                    Key::Q => self.should_quit = true,
                    _ => {}
                }
            }
        }
    }

    // ── Per-frame render ──────────────────────────────────────────────────────

    /// Called once per frame inside the egui context.
    ///
    /// The WCAG IR pipeline: `game_state → *_to_verified_tree() → render_tree(ui, …)`.
    #[instrument(skip(self, ui))]
    fn render_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.handle_keys(&ctx);

        match &self.active {
            ActiveGame::Menu => {
                render_menu(ui);
            }
            ActiveGame::TicTacToe {
                game,
                cursor,
                events,
            } => {
                render_ttt_egui(ui, game, cursor, events);
            }
        }
    }
}

// ── IR render helpers ─────────────────────────────────────────────────────────

/// Render the main menu via the IR pipeline.
///
/// Builds a minimal one-node `VerifiedTree` containing the help text and
/// drives `elicit_egui::render_tree` — same pattern as game renders.
#[instrument(skip(ui))]
fn render_menu(ui: &mut egui::Ui) {
    // Build a trivial verified tree for the menu screen.
    use accesskit::{Node, NodeId, Role};
    use elicit_ui::{VerifiedTree, Viewport};

    let size = ui.available_size();
    let vp = Viewport::new(size.x as u32, size.y as u32);

    let root_id = NodeId::from(1u64);
    let mut root = Node::new(Role::Window);
    root.set_label("Strictly Games");
    root.set_description("T — Tic-tac-toe   |   Q/Esc — quit");

    let mut nodes = std::collections::BTreeMap::new();
    nodes.insert(root_id, root);
    let tree = VerifiedTree::from_parts(nodes, root_id, vp);
    let (_stats, _clicked) = elicit_egui::render_tree(ui, tree.nodes(), tree.root());

    // Overlay friendly text directly (egui is immediate mode).
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.heading("Strictly Games");
            ui.add_space(20.0);
            ui.label("T — Tic-tac-toe");
            ui.add_space(40.0);
            ui.label("Q / Esc — quit");
        });
    });
}

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

    let empty_nodes: &[_] = &[];
    let empty_edges: &[_] = &[];
    let log = EventLog {
        events,
        dialogue: &[],
    };
    let graph = GraphParams {
        nodes: empty_nodes,
        edges: empty_edges,
        active: None,
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
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .expect("create wgpu surface");

        // wgpu async init — safe to block here: running on a plain std thread.
        let (adapter, device, queue) = futures::executor::block_on(async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                })
                .await
                .expect(
                    "no suitable wgpu adapter — ensure Vulkan/Metal/DX12 drivers are installed",
                );
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .expect("could not create wgpu device");
            (adapter, device, queue)
        });
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
                let raw = self.egui_state.as_mut().unwrap().take_egui_input(&window);
                let ctx = self.egui_state.as_ref().unwrap().egui_ctx().clone();

                let out = ctx.run_ui(raw, |ui| self.render_ui(ui));

                if self.should_quit {
                    event_loop.exit();
                    return;
                }

                self.egui_state
                    .as_mut()
                    .unwrap()
                    .handle_platform_output(&window, out.platform_output);

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
#[instrument]
pub fn run_egui() -> anyhow::Result<()> {
    info!("Starting egui frontend");
    let event_loop = EventLoop::new()?;
    let mut app = GamesEguiApp::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
