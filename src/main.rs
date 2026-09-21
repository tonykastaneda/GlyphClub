use std::sync::Arc;

use vello::kurbo::{Affine, Point};
use vello::util::RenderSurface;
use vello::Scene;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

mod activate;
mod app;
mod catalog;
mod font;
mod gpu;
mod reveal;
mod scan;
mod store;
mod text;
mod theme;
#[cfg(target_os = "macos")]
mod traffic_lights;
mod ui;

use app::{App, Focus};
use catalog::Catalog;
use gpu::Gpu;
use text::TextCx;

struct WindowState {
    window: Arc<Window>,
    surface: RenderSurface<'static>,
}

struct Shell {
    gpu: Gpu,
    text: TextCx,
    state: Option<WindowState>,
    app: App,
    cursor: Point,
    modifiers: ModifiersState,
}

impl Shell {
    fn new() -> Self {
        let catalog = Catalog::open().expect("open catalog database");
        Self {
            gpu: Gpu::new(),
            text: TextCx::new(),
            state: None,
            app: App::new(catalog),
            cursor: Point::ZERO,
            modifiers: ModifiersState::empty(),
        }
    }
}

fn focused_string(app: &mut App) -> Option<&mut String> {
    match app.focus {
        Focus::Search => Some(&mut app.search),
        Focus::PreviewText => Some(&mut app.preview_text),
        Focus::SampleText => Some(&mut app.sample_text),
        Focus::None => None,
    }
}

/// Takes `app` directly rather than `&mut Shell` so it borrows only the
/// `app` field — the caller still holds a live borrow of `state` (a
/// different field) across this call.
fn handle_key(app: &mut App, event: winit::event::KeyEvent, modifiers: ModifiersState) {
    if modifiers.super_key() {
        if let Key::Character(key) = &event.logical_key {
            if key.eq_ignore_ascii_case("r") {
                set_selected_activation(app, !modifiers.shift_key());
                return;
            }
        }
    }
    if app.focus == Focus::None {
        return;
    }
    match event.logical_key {
        Key::Named(NamedKey::Backspace) => {
            let is_search = app.focus == Focus::Search;
            if let Some(s) = focused_string(app) {
                s.pop();
            }
            if is_search {
                app.reload_fonts();
            }
        }
        Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Enter) => {
            app.focus = Focus::None;
        }
        _ => {
            if let Some(txt) = event.text {
                let is_search = app.focus == Focus::Search;
                if let Some(s) = focused_string(app) {
                    for ch in txt.chars().filter(|c| !c.is_control()) {
                        s.push(ch);
                    }
                }
                if is_search {
                    app.reload_fonts();
                }
            }
        }
    }
}

/// Custom close/minimize/zoom circles drawn in the sidebar (`ui::sidebar`)
/// need their own dispatch, separate from the generic `ui::handle_click` —
/// these are window-chrome actions that need `event_loop`/`window`
/// directly, not app state. Returns whether the click was on one of them
/// (the caller skips the generic dispatch when it was).
#[cfg(target_os = "macos")]
fn handle_traffic_light_click(
    app: &App,
    window: &Window,
    event_loop: &ActiveEventLoop,
    point: Point,
) -> bool {
    let [close, minimize, zoom] = app.traffic_light_rects;
    if close.contains(point) {
        event_loop.exit();
        true
    } else if minimize.contains(point) {
        window.set_minimized(true);
        window.request_redraw();
        true
    } else if zoom.contains(point) {
        window.set_maximized(!window.is_maximized());
        window.request_redraw();
        true
    } else {
        false
    }
}

fn set_selected_activation(app: &mut App, should_activate: bool) {
    let Some(id) = app.selected else { return };
    let is_system = app
        .entries
        .iter()
        .any(|entry| entry.id == id && entry.is_system);
    if is_system || app.active_ids.contains(&id) == should_activate {
        return;
    }
    if let Ok(active) = app.catalog.toggle_activation(id) {
        if active {
            app.active_ids.insert(id);
        } else {
            app.active_ids.remove(&id);
        }
    }
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("GlyphClub")
            .with_inner_size(winit::dpi::LogicalSize::new(1360.0, 860.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(760.0, 480.0));
        // macOS: paint behind the title bar and drop its text so the
        // sidebar's own background can run up to the top of the window,
        // with only the traffic lights sitting on top of it. Windows keeps
        // its native title bar as-is — its window controls live top-right,
        // not stacked over app content the way the traffic lights are.
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true)
                .with_title_hidden(true)
        };
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        #[cfg(target_os = "macos")]
        traffic_lights::hide_native_buttons(&window);
        let surface = self.gpu.create_surface(window.clone());
        window.request_redraw();
        self.state = Some(WindowState { window, surface });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else { return };
        if state.window.id() != id {
            return;
        }
        let scale = state.window.scale_factor();

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.gpu.resize(&mut state.surface, size.width, size.height);
                state.window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Point::new(position.x / scale, position.y / scale);
                self.app.hover = self.cursor;
                if self.app.dragging_tile_size {
                    ui::update_tile_size_from_point(&mut self.app, self.cursor);
                }
                if self.app.dragging_scrollbar {
                    ui::update_scroll_from_point(&mut self.app, self.cursor);
                }
                state.window.request_redraw();
            }
            WindowEvent::CursorLeft { .. } => {
                self.app.hover = Point::new(-1.0, -1.0);
                state.window.request_redraw();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64 * 44.0,
                    MouseScrollDelta::PixelDelta(p) => p.y / scale,
                };
                self.app.scroll_y -= dy;
                if self.app.scroll_y < 0.0 {
                    self.app.scroll_y = 0.0;
                }
                state.window.request_redraw();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                #[cfg(target_os = "macos")]
                if handle_traffic_light_click(&self.app, &state.window, event_loop, self.cursor) {
                    return;
                }
                ui::handle_click(&mut self.app, self.cursor);
                state.window.request_redraw();
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                self.app.dragging_tile_size = false;
                self.app.dragging_scrollbar = false;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                ui::handle_right_click(&mut self.app, self.cursor);
                state.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                handle_key(&mut self.app, event, self.modifiers);
                state.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                // Idempotent and cheap, so reapplying every redraw is fine
                // — a safety net in case an earlier call landed before
                // AppKit had actually created the buttons.
                #[cfg(target_os = "macos")]
                traffic_lights::hide_native_buttons(&state.window);
                let size = state.window.inner_size();
                let (lw, lh) = (size.width as f64 / scale, size.height as f64 / scale);
                let content = ui::draw(&mut self.app, &mut self.text, lw, lh);

                let mut root = Scene::new();
                root.append(&content, Some(Affine::scale(scale)));
                self.gpu
                    .render_and_present(&mut state.surface, &root, theme::CANVAS);

                // No continuous-redraw loop: idle until an input event
                // changes something. Text-field carets don't blink for now.
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut shell = Shell::new();
    event_loop.run_app(&mut shell).expect("run event loop");
}
