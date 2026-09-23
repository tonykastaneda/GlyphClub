use std::sync::Arc;
use std::time::{Duration, Instant};

use vello::kurbo::{Affine, Point};
use vello::util::RenderSurface;
use vello::Scene;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

mod activate;
mod activation_memory;
mod app;
mod branding;
mod catalog;
mod font;
mod font_info;
mod gpu;
#[cfg(target_os = "macos")]
mod native_menu;
mod reveal;
mod scan;
mod settings;
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

/// The 1024px master icon export (see `branding/SVG/icon.svg` for the
/// vector source), embedded directly into the binary so the real app icon
/// works even for a bare `cargo run`/`cargo build` build with no
/// `Info.plist`/`.icns`/`.ico` packaging step yet.
const BRAND_ICON_PNG: &[u8] = include_bytes!("../branding/glyphicon-macOS-Default-1024@1x.png");

/// Decodes `BRAND_ICON_PNG` into the RGBA buffer winit's taskbar/title-bar
/// icon wants. Returns `None` on a decode failure rather than panicking —
/// worst case the window just keeps whatever default icon the OS assigns.
#[cfg(target_os = "windows")]
fn load_window_icon() -> Option<winit::window::Icon> {
    let img = image::load_from_memory(BRAND_ICON_PNG).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), width, height).ok()
}

/// Fired by the catalog's background filesystem watcher (see
/// `Catalog::open`) to hop back onto the event loop's own thread — winit's
/// `EventLoopProxy` is the one thread-safe way in, since `App`/`Shell`
/// otherwise only ever get touched from the main thread.
pub(crate) enum AppEvent {
    FoldersChanged,
    /// A background font-bytes prefetch (see `app::FontBytesPrefetcher`)
    /// finished loading a file that's still on screen — just needs a
    /// repaint to pick up the now-ready data, not a full `reload_fonts`
    /// (which would also reset `scroll_y`, wrong mid-scroll).
    Redraw,
}

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
    #[cfg(target_os = "macos")]
    native_menu: Option<native_menu::NativeMenu>,
}

impl Shell {
    fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
        let watcher_proxy = proxy.clone();
        let catalog = Catalog::open(move || {
            let _ = watcher_proxy.send_event(AppEvent::FoldersChanged);
        })
        .expect("open catalog database");
        Self {
            gpu: Gpu::new(),
            text: TextCx::new(),
            state: None,
            app: App::new(catalog, proxy),
            cursor: Point::ZERO,
            modifiers: ModifiersState::empty(),
            #[cfg(target_os = "macos")]
            native_menu: None,
        }
    }
}

/// Applies one native-menu click. Kept free of `event_loop`/`window` on
/// Handles every `MenuAction` except `Quit`, which needs `event_loop` to
/// actually exit — that one's intercepted in `about_to_wait`'s drain loop
/// before it would otherwise reach here. Everything real routes through
/// the same `ui::` functions the hand-rolled Windows menu calls too, so
/// there's exactly one implementation of each Font-menu action, not two
/// copies drifting apart.
#[cfg(target_os = "macos")]
fn dispatch_menu_action(app: &mut App, action: native_menu::MenuAction) {
    use native_menu::MenuAction;
    match action {
        MenuAction::AddLibrary => ui::add_library(app),
        MenuAction::Sync => ui::sync_now(app),
        MenuAction::FindFocus => app.focus = Focus::Search,
        MenuAction::SetView(mode) => {
            app.view_mode = mode;
            app.scroll_y = 0.0;
        }
        MenuAction::ToggleInfo => app.detail_open = !app.detail_open,
        MenuAction::ZoomIn => app.tile_size = (app.tile_size + 20.0).min(theme::MAX_TILE),
        MenuAction::ZoomOut => app.tile_size = (app.tile_size - 20.0).max(theme::MIN_TILE),
        MenuAction::Activate => ui::activate_selected(app),
        MenuAction::ActivateTemporarily => ui::activate_selected_temporarily(app),
        MenuAction::Deactivate => ui::deactivate_selected(app),
        MenuAction::ToggleFavorite => ui::toggle_selected_favorite(app),
        MenuAction::ExportSelected => ui::export_selected(app),
        MenuAction::RevealSelected => ui::reveal_selected(app),
        MenuAction::RemoveFromFontlist => ui::remove_selected_from_fontlist(app),
        MenuAction::DeleteSelected => ui::request_delete_selected(app),
        MenuAction::OpenGitHub => ui::open_github_repo(),
        MenuAction::OpenSettings => app.settings_open = true,
        MenuAction::Quit => unreachable!("intercepted in about_to_wait before dispatch"),
    }
}

/// `Focus::PreviewText` is deliberately absent — `App::preview_editor` is a
/// real `parley::PlainEditor`, not a plain `String`, and is driven directly
/// in `handle_key`/the mouse handlers below instead of through this
/// generic append/backspace-at-the-end mechanism.
fn focused_string(app: &mut App) -> Option<&mut String> {
    match app.focus {
        Focus::Search => Some(&mut app.search),
        Focus::PreviewText => None,
        Focus::SampleText => Some(&mut app.sample_text),
        Focus::GlyphSearch => Some(&mut app.glyph_search),
        Focus::PreviewSize => Some(&mut app.preview_size_input),
        // Handled separately in `handle_key`, same as `PreviewText` — it
        // lives inside `Option<FontContextMenu>`, not a plain field this
        // generic accessor can hand back a `&mut String` for.
        Focus::NewTag => None,
        Focus::None => None,
    }
}

/// Converts a window-space point (`app.cursor`'s own space) into
/// `App::preview_editor`'s local layout coordinates — its top-left is
/// `preview_content_rect`'s origin, scrolled by `detail_tab_scroll_y` (see
/// `ui::detail::draw_preview_tab`, which renders it with exactly this same
/// offset). `None` outside the live-type area or before it's ever drawn.
fn preview_local_point(app: &App, point: Point) -> Option<Point> {
    let rect = app.preview_content_rect?;
    if !rect.contains(point) {
        return None;
    }
    Some(Point::new(point.x - rect.x0, point.y - rect.y0 + app.detail_tab_scroll_y))
}

/// Takes `app`/`text` directly rather than `&mut Shell` so it only borrows
/// those two fields — the caller still holds a live borrow of `state` (a
/// different field) across this call.
fn handle_key(app: &mut App, text: &mut TextCx, event: winit::event::KeyEvent, modifiers: ModifiersState) {
    // On macOS this would double-fire alongside the native menu bar's own
    // ⌘R/⇧⌘R accelerators (see `native_menu.rs`), which own it there
    // instead. Windows has no native menu yet, so its primary modifier —
    // Ctrl, not the Windows/Meta key `super_key()` actually means — still
    // routes through here.
    #[cfg(target_os = "macos")]
    let _ = &modifiers;
    #[cfg(not(target_os = "macos"))]
    if modifiers.control_key() {
        if let Key::Character(key) = &event.logical_key {
            if key.eq_ignore_ascii_case("r") {
                if modifiers.shift_key() {
                    ui::deactivate_selected(app);
                } else {
                    ui::activate_selected(app);
                }
                return;
            }
        }
    }
    if app.focus == Focus::None {
        return;
    }

    // The Tags submenu's "New Tag…" field — Enter commits it (creates and
    // applies the tag), unlike every other field's Enter, which just
    // defocuses. Handled before the generic dispatch below for the same
    // reason `PreviewText` is.
    if app.focus == Focus::NewTag {
        match event.logical_key {
            Key::Named(NamedKey::Escape) => {
                if let Some(m) = app.font_context_menu.as_mut() {
                    m.new_tag_text = None;
                }
                app.focus = Focus::None;
            }
            Key::Named(NamedKey::Enter) => ui::confirm_new_tag(app),
            Key::Named(NamedKey::Backspace) => {
                if let Some(s) = app.font_context_menu.as_mut().and_then(|m| m.new_tag_text.as_mut()) {
                    s.pop();
                }
            }
            _ => {
                if let Some(txt) = event.text {
                    if let Some(s) = app.font_context_menu.as_mut().and_then(|m| m.new_tag_text.as_mut()) {
                        for ch in txt.chars().filter(|c| !c.is_control()) {
                            s.push(ch);
                        }
                    }
                }
            }
        }
        return;
    }

    // `App::preview_editor` is a real `parley::PlainEditor` (cursor,
    // selection, word/line/paragraph navigation) rather than a plain
    // `String`, so it's driven separately from every other
    // (plain-`String`) focusable field below.
    if app.focus == Focus::PreviewText {
        handle_preview_key(app, text, &event, modifiers);
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

/// `Focus::PreviewText`'s own key handling — arrow-key cursor movement and
/// selection (word/line/paragraph, per the user's spec), Backspace,
/// Enter (a literal newline — real paragraph breaks in the specimen, not
/// "done, defocus" like every other field), Escape, and plain typing, all
/// driven through `App::preview_editor`'s `parley::PlainEditorDriver`
/// rather than the generic `focused_string`/append-at-end path.
///
/// Modifier mapping (matches what parley's driver methods are each named
/// for): plain arrows move by character/line; Shift extends selection the
/// same way; Option/Alt+Left/Right moves by word; Cmd *or* Ctrl+Left/Right
/// moves to the start/end of the current *visual* (wrapped) line; Option+
/// Shift+Up/Down selects to the start/end of the current *paragraph*
/// (parley calls this the "hard line", i.e. `\n`-delimited); Cmd/Ctrl+
/// Shift+Left/Right selects to the start/end of the visual line. Option+
/// Shift+Left/Right (word-selection) is included too — not explicitly
/// requested, but the standard pairing with plain Option+Left/Right and
/// trivial given the API.
fn handle_preview_key(app: &mut App, text: &mut TextCx, event: &winit::event::KeyEvent, modifiers: ModifiersState) {
    let shift = modifiers.shift_key();
    let alt = modifiers.alt_key();
    // The user's own spec says "cmd or ctrl" rather than splitting by
    // platform — unlike the global Activate/Deactivate shortcut above,
    // this is a plain in-field edit command with no native-menu
    // accelerator to collide with, so there's no reason to gate it by OS.
    let primary = modifiers.super_key() || modifiers.control_key();

    match &event.logical_key {
        Key::Named(NamedKey::Escape) => {
            app.focus = Focus::None;
        }
        Key::Named(NamedKey::Backspace) => {
            text.preview_driver(&mut app.preview_editor).backdelete();
        }
        Key::Named(NamedKey::Enter) => {
            text.preview_driver(&mut app.preview_editor).insert_or_replace_selection("\n");
        }
        Key::Named(NamedKey::ArrowLeft) => {
            let mut driver = text.preview_driver(&mut app.preview_editor);
            match (primary, alt, shift) {
                (true, _, true) => driver.select_to_line_start(),
                (true, _, false) => driver.move_to_line_start(),
                (false, true, true) => driver.select_word_left(),
                (false, true, false) => driver.move_word_left(),
                (false, false, true) => driver.select_left(),
                (false, false, false) => driver.move_left(),
            }
        }
        Key::Named(NamedKey::ArrowRight) => {
            let mut driver = text.preview_driver(&mut app.preview_editor);
            match (primary, alt, shift) {
                (true, _, true) => driver.select_to_line_end(),
                (true, _, false) => driver.move_to_line_end(),
                (false, true, true) => driver.select_word_right(),
                (false, true, false) => driver.move_word_right(),
                (false, false, true) => driver.select_right(),
                (false, false, false) => driver.move_right(),
            }
        }
        Key::Named(NamedKey::ArrowUp) => {
            let mut driver = text.preview_driver(&mut app.preview_editor);
            match (alt, shift) {
                (true, true) => driver.select_to_hard_line_start(),
                (_, true) => driver.select_up(),
                _ => driver.move_up(),
            }
        }
        Key::Named(NamedKey::ArrowDown) => {
            let mut driver = text.preview_driver(&mut app.preview_editor);
            match (alt, shift) {
                (true, true) => driver.select_to_hard_line_end(),
                (_, true) => driver.select_down(),
                _ => driver.move_down(),
            }
        }
        _ => {
            if let Some(txt) = &event.text {
                let filtered: String = txt.chars().filter(|c| !c.is_control()).collect();
                if !filtered.is_empty() {
                    text.preview_driver(&mut app.preview_editor).insert_or_replace_selection(&filtered);
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
    app: &mut App,
    window: &Window,
    event_loop: &ActiveEventLoop,
    point: Point,
) -> bool {
    let [close, minimize, zoom] = app.traffic_light_rects;
    if close.contains(point) {
        ui::cleanup_before_quit(app);
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

impl ApplicationHandler<AppEvent> for Shell {
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
        // Resolves `Appearance::SystemDefault` against the OS's actual
        // current preference — `App::new` has no `Window` yet to ask this
        // of, so it just guesses dark; this is the first real answer, and
        // `WindowEvent::ThemeChanged` (below) keeps it current after.
        if let Some(theme) = window.theme() {
            self.app.system_is_dark = theme == winit::window::Theme::Dark;
        }
        self.app.apply_theme();
        #[cfg(target_os = "macos")]
        {
            traffic_lights::hide_native_buttons(&window);
            traffic_lights::disable_titlebar_drag(&window);
            traffic_lights::set_dock_icon(BRAND_ICON_PNG);
            self.native_menu = Some(native_menu::NativeMenu::build(self.app.view_mode));
        }
        #[cfg(target_os = "windows")]
        if let Some(icon) = load_window_icon() {
            window.set_window_icon(Some(icon));
        }
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
            WindowEvent::CloseRequested => {
                ui::cleanup_before_quit(&mut self.app);
                event_loop.exit();
            }
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
                if self.app.header_press.is_some() || self.app.dragging_column_reorder.is_some() {
                    ui::update_column_drag_from_point(&mut self.app, self.cursor);
                }
                if self.app.column_resize_start.is_some() {
                    ui::update_column_resize_from_point(&mut self.app, self.cursor);
                }
                if self.app.dragging_detail_resize {
                    let window_width = state.window.inner_size().width as f64 / scale;
                    ui::update_detail_resize_from_point(&mut self.app, self.cursor, window_width);
                }
                if self.app.dragging_preview_selection {
                    if let Some(local) = preview_local_point(&self.app, self.cursor) {
                        self.text
                            .preview_driver(&mut self.app.preview_editor)
                            .extend_selection_to_point(local.x as f32, local.y as f32);
                    }
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
            WindowEvent::ThemeChanged(theme) => {
                self.app.system_is_dark = theme == winit::window::Theme::Dark;
                if self.app.settings.appearance == settings::Appearance::SystemDefault {
                    self.app.apply_theme();
                    state.window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64 * 44.0,
                    MouseScrollDelta::PixelDelta(p) => p.y / scale,
                };
                let size = state.window.inner_size();
                let width = size.width as f64 / scale;
                let over_detail = self.app.detail_open
                    && self.app.selected.is_some()
                    && self.cursor.x >= width - self.app.detail_w;
                if over_detail {
                    // Clamped against `detail_tab_scroll_max` right here,
                    // not just left to the next redraw's own clamp — a
                    // fast scroll gesture fires a burst of wheel events
                    // between redraws, and without this, the value could
                    // shoot hundreds of pixels past the real max before
                    // the next redraw yanked it back, which is what
                    // showed up as "scrolls, then snaps back".
                    self.app.detail_tab_scroll_y =
                        (self.app.detail_tab_scroll_y - dy).clamp(0.0, self.app.detail_tab_scroll_max);
                } else {
                    self.app.scroll_y -= dy;
                    if self.app.scroll_y < 0.0 {
                        self.app.scroll_y = 0.0;
                    }
                }
                state.window.request_redraw();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                #[cfg(target_os = "macos")]
                if handle_traffic_light_click(&mut self.app, &state.window, event_loop, self.cursor) {
                    return;
                }
                #[cfg(target_os = "macos")]
                {
                    // AppKit's own title-bar-strip window-drag is
                    // unconditionally disabled (see `traffic_lights::
                    // disable_titlebar_drag`), since it can't be made
                    // conditional on *this* click's position — so a press
                    // that missed every control re-implements "drag the
                    // window from empty background" by hand instead, the
                    // same way a native titlebar would.
                    if !ui::handle_click(&mut self.app, self.cursor) {
                        let result = state.window.drag_window();
                        eprintln!(
                            "[drag] BACKGROUND click -> drag_window() cursor={:?} detail_open={} selected={:?} result={:?}",
                            self.cursor, self.app.detail_open, self.app.selected, result
                        );
                    } else {
                        eprintln!(
                            "[drag] CONSUMED cursor={:?} detail_open={} focus={:?}",
                            self.cursor, self.app.detail_open, self.app.focus
                        );
                    }
                }
                #[cfg(not(target_os = "macos"))]
                ui::handle_click(&mut self.app, self.cursor);
                // `ui::handle_click` just set `app.focus` from whatever hit
                // region the click landed on — if that was the Preview
                // tab's live-type area, position the editor's own
                // cursor/selection there too (a plain click moves it,
                // Shift-click extends the existing selection to it), and
                // arm drag-to-select for any `CursorMoved` that follows
                // before release.
                if self.app.focus == Focus::PreviewText {
                    if let Some(local) = preview_local_point(&self.app, self.cursor) {
                        let mut driver = self.text.preview_driver(&mut self.app.preview_editor);
                        if self.modifiers.shift_key() {
                            driver.shift_click_extension(local.x as f32, local.y as f32);
                        } else {
                            driver.move_to_point(local.x as f32, local.y as f32);
                        }
                        self.app.dragging_preview_selection = true;
                    }
                }
                state.window.request_redraw();
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                self.app.dragging_tile_size = false;
                self.app.dragging_scrollbar = false;
                self.app.dragging_detail_resize = false;
                self.app.dragging_preview_selection = false;
                if self.app.header_press.is_some()
                    || self.app.dragging_column_reorder.is_some()
                    || self.app.column_resize_start.is_some()
                {
                    ui::finish_header_interaction(&mut self.app);
                    state.window.request_redraw();
                }
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
                handle_key(&mut self.app, &mut self.text, event, self.modifiers);
                state.window.request_redraw();
            }
            WindowEvent::HoveredFile(_) => {
                if !self.app.drag_hovering {
                    self.app.drag_hover_since = Some(Instant::now());
                }
                self.app.drag_hovering = true;
                // Keeps redraws flowing for the fan-out animation's
                // duration — `about_to_wait` drops this back to `Wait`
                // once it settles (matching the drop-flash pattern above).
                event_loop.set_control_flow(ControlFlow::Poll);
                state.window.request_redraw();
            }
            WindowEvent::HoveredFileCancelled => {
                self.app.drag_hovering = false;
                self.app.drag_hover_since = None;
                state.window.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                self.app.drag_hovering = false;
                self.app.drag_hover_since = None;
                self.app.pending_drop = Some(path);
                self.app.drop_flash_until = Some(Instant::now() + Duration::from_millis(350));
                event_loop.set_control_flow(ControlFlow::Poll);
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
                    .render_and_present(&mut state.surface, &root, theme::CANVAS());

                // No continuous-redraw loop otherwise: idle until an input
                // event changes something. Text-field carets don't blink
                // for now. The one exception is the drop-flash overlay,
                // which self-expires on a timer rather than an input event
                // — `about_to_wait` below keeps redraws flowing for that.
            }
            _ => {}
        }
    }

    /// Keeps redraws flowing for as long as something is mid-animation on
    /// a timer rather than an input event — the drop flash
    /// (`drop_flash_until`, armed by `WindowEvent::DroppedFile`) and the
    /// drag-hover cards' fan-out (`drag_hover_since`, armed by
    /// `WindowEvent::HoveredFile`) — then drops `control_flow` back to
    /// `Wait` once neither needs it anymore.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.native_menu {
            let actions = menu.drain();
            if !actions.is_empty() {
                for action in actions {
                    // Quit needs `event_loop` (to actually exit) and to
                    // run its cleanup before anything else does — handled
                    // here instead of in `dispatch_menu_action`, which
                    // only ever sees the rest.
                    if matches!(action, native_menu::MenuAction::Quit) {
                        ui::cleanup_before_quit(&mut self.app);
                        event_loop.exit();
                        return;
                    }
                    dispatch_menu_action(&mut self.app, action);
                }
                menu.sync_view(self.app.view_mode);
                if let Some(state) = &self.state {
                    state.window.request_redraw();
                }
            }
        }

        let flash_active = match self.app.drop_flash_until {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                self.app.drop_flash_until = None;
                false
            }
            None => false,
        };
        let fan_animating = self.app.drag_hovering
            && self
                .app
                .drag_hover_since
                .is_some_and(|since| since.elapsed().as_secs_f64() < ui::DRAG_HOVER_FAN_SECS);

        if !flash_active && !fan_animating {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::FoldersChanged => {
                if let Ok(result) = self.app.catalog.rescan() {
                    self.app.reload_folders_and_fonts(result.status);
                }
                if let Some(state) = &self.state {
                    state.window.request_redraw();
                }
            }
            AppEvent::Redraw => {
                if let Some(state) = &self.state {
                    state.window.request_redraw();
                }
            }
        }
    }
}

fn main() {
    // Hard-disables the `log` facade before anything (parley included)
    // gets a chance to use it — see the dependency comment in Cargo.toml.
    // This is a blanket "we don't consume logs" cutoff, not a targeted fix
    // for one noisy crate, since `set_max_level` filters at the facade
    // itself: nothing downstream even builds a log record, let alone
    // formats or writes one, regardless of which crate calls `log::warn!`.
    log::set_max_level(log::LevelFilter::Off);

    // As early as possible — before `EventLoop`/`NSApplication` even
    // exist — since these only reliably suppress AppKit's automatic
    // Edit-menu items (see `native_menu.rs`) when set well ahead of
    // whenever it decides to inject them, which in testing turned out to
    // be earlier than building the menu itself in `resumed()`.
    #[cfg(target_os = "macos")]
    native_menu::suppress_system_edit_menu_items();

    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut shell = Shell::new(event_loop.create_proxy());
    event_loop.run_app(&mut shell).expect("run event loop");
}
