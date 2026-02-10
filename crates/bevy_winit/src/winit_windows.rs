use bevy_ecs::entity::Entity;

use bevy_ecs::entity::EntityHashMap;
use bevy_platform::collections::HashMap;
use bevy_window::{
    CursorGrabMode, CursorOptions, MonitorSelection, VideoModeSelection, Window, WindowMode,
    WindowPosition, WindowResolution, WindowWrapper,
};
use tracing::warn;

use winit::{
    dpi::{LogicalSize, PhysicalPosition},
    error::RequestError,
    event_loop::ActiveEventLoop,
    monitor::{Fullscreen, MonitorHandle, VideoMode},
    window::{CursorGrabMode as WinitCursorGrabMode, Window as WinitWindow, WindowId},
};

use crate::{
    converters::{convert_enabled_buttons, convert_window_level, convert_window_theme},
    winit_monitors::WinitMonitors,
};

/// A resource mapping window entities to their `winit`-backend [`Window`](winit::window::Window)
/// states.
#[derive(Debug, Default)]
pub struct WinitWindows {
    /// Stores [`winit`] windows by window identifier.
    pub windows: HashMap<WindowId, WindowWrapper<Box<dyn WinitWindow>>>,
    /// Maps entities to `winit` window identifiers.
    pub entity_to_winit: EntityHashMap<WindowId>,
    /// Maps `winit` window identifiers to entities.
    pub winit_to_entity: HashMap<WindowId, Entity>,
    // Many `winit` window functions (e.g. `set_window_icon`) can only be called on the main thread.
    // If they're called on other threads, the program might hang. This marker indicates that this
    // type is not thread-safe and will be `!Send` and `!Sync`.
    _not_send_sync: core::marker::PhantomData<*const ()>,
}

impl WinitWindows {
    /// Creates a new instance of `WinitWindows`.
    pub const fn new() -> Self {
        Self {
            windows: HashMap::new(),
            entity_to_winit: EntityHashMap::new(),
            winit_to_entity: HashMap::new(),
            _not_send_sync: core::marker::PhantomData,
        }
    }

    /// Creates a `winit` window and associates it with our entity.
    pub fn create_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        entity: Entity,
        window: &Window,
        cursor_options: &CursorOptions,
        monitors: &WinitMonitors,
    ) -> Result<&WindowWrapper<Box<dyn WinitWindow>>, RequestError> {
        // WinRT/UWP is limited to a single CoreWindow in the initial backend. winit will return
        // NotSupported on additional window creation attempts; proactively enforce that here to
        // avoid panics elsewhere.
        #[cfg(all(target_os = "windows", __WINRT__))]
        if !self.windows.is_empty() {
            return Err(
                winit::error::NotSupportedError::new("WinRT/UWP supports only a single window")
                    .into(),
            );
        }

        let mut winit_window_attributes = winit::window::WindowAttributes::default();

        let maybe_selected_monitor = {
            // WinRT/UWP only supports a single synthetic monitor in the initial backend and
            // monitor queries can fail early during app boot on some devices. Avoid calling
            // monitor APIs here: borderless fullscreen does not require an explicit monitor.
            #[cfg(all(target_os = "windows", __WINRT__))]
            {
                None
            }

            #[cfg(not(all(target_os = "windows", __WINRT__)))]
            {
                match window.mode {
                    WindowMode::BorderlessFullscreen(monitor_selection)
                    | WindowMode::Fullscreen(monitor_selection, _) => select_monitor(
                        monitors,
                        event_loop.primary_monitor(),
                        None,
                        &monitor_selection,
                    ),
                    WindowMode::Windowed => None,
                }
            }
        };

        winit_window_attributes = match window.mode {
            WindowMode::BorderlessFullscreen(_) => winit_window_attributes
                .with_fullscreen(Some(Fullscreen::Borderless(maybe_selected_monitor.clone()))),
            WindowMode::Fullscreen(monitor_selection, video_mode_selection) => {
                if let Some(select_monitor) = maybe_selected_monitor.clone() {
                    if let Some(video_mode) =
                        get_selected_videomode(&select_monitor, &video_mode_selection)
                    {
                        winit_window_attributes.with_fullscreen(Some(Fullscreen::Exclusive(
                            select_monitor.clone(),
                            video_mode,
                        )))
                    } else {
                        warn!(
                            "Could not find valid fullscreen video mode for {:?} {:?}",
                            monitor_selection, video_mode_selection
                        );
                        winit_window_attributes
                    }
                } else {
                    warn!(
                        "Could not find monitor for {monitor_selection:?}; falling back to borderless fullscreen"
                    );
                    winit_window_attributes.with_fullscreen(Some(Fullscreen::Borderless(None)))
                }
            }
            WindowMode::Windowed => {
                #[cfg(not(all(target_os = "windows", __WINRT__)))]
                {
                    if let Some(position) = winit_window_position(
                        &window.position,
                        &window.resolution,
                        monitors,
                        event_loop.primary_monitor(),
                        None,
                    ) {
                        winit_window_attributes = winit_window_attributes.with_position(position);
                    }
                }
                let logical_size = LogicalSize::new(window.width(), window.height());
                if let Some(sf) = window.resolution.scale_factor_override() {
                    let inner_size = logical_size.to_physical::<f64>(sf.into());
                    winit_window_attributes.with_surface_size(inner_size)
                } else {
                    winit_window_attributes.with_surface_size(logical_size)
                }
            }
        };

        winit_window_attributes = winit_window_attributes
            .with_window_level(convert_window_level(window.window_level))
            .with_theme(window.window_theme.map(convert_window_theme))
            .with_resizable(window.resizable)
            .with_enabled_buttons(convert_enabled_buttons(window.enabled_buttons))
            .with_decorations(window.decorations)
            .with_transparent(window.transparent)
            .with_active(window.focused);

        #[cfg(all(target_os = "windows", not(__WINRT__)))]
        {
            use winit::platform::windows::WindowAttributesWindows;

            let win_attributes = WindowAttributesWindows::default()
                .with_skip_taskbar(window.skip_taskbar)
                .with_clip_children(window.clip_children);
            winit_window_attributes =
                winit_window_attributes.with_platform_attributes(Box::new(win_attributes));
        }

        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesMacOS;

            // macOS-specific attributes are configured via an opaque platform attributes object.
            let macos_attributes = WindowAttributesMacOS::default()
                .with_movable_by_window_background(window.movable_by_window_background)
                .with_fullsize_content_view(window.fullsize_content_view)
                .with_has_shadow(window.has_shadow)
                .with_titlebar_hidden(!window.titlebar_shown)
                .with_titlebar_transparent(window.titlebar_transparent)
                .with_title_hidden(!window.titlebar_show_title)
                .with_titlebar_buttons_hidden(!window.titlebar_show_buttons);

            winit_window_attributes =
                winit_window_attributes.with_platform_attributes(Box::new(macos_attributes));
        }

        #[cfg(target_os = "ios")]
        {
            use crate::converters::convert_screen_edge;
            use winit::platform::ios::WindowAttributesExtIOS;

            let preferred_edge =
                convert_screen_edge(window.preferred_screen_edges_deferring_system_gestures);

            winit_window_attributes = winit_window_attributes
                .with_preferred_screen_edges_deferring_system_gestures(preferred_edge);
            winit_window_attributes = winit_window_attributes
                .with_prefers_home_indicator_hidden(window.prefers_home_indicator_hidden);
            winit_window_attributes = winit_window_attributes
                .with_prefers_status_bar_hidden(window.prefers_status_bar_hidden);
        }

        let display_info = DisplayInfo {
            window_physical_resolution: (
                window.resolution.physical_width(),
                window.resolution.physical_height(),
            ),
            window_logical_resolution: (window.resolution.width(), window.resolution.height()),
            monitor_name: maybe_selected_monitor
                .as_ref()
                .and_then(|m| m.name().map(|n| n.into_owned())),
            scale_factor: maybe_selected_monitor
                .as_ref()
                .map(|m| m.scale_factor()),
            refresh_rate_millihertz: maybe_selected_monitor
                .as_ref()
                .and_then(|m| m.current_video_mode())
                .and_then(|vm| vm.refresh_rate_millihertz().map(|r| r.get())),
        };
        bevy_log::debug!("{display_info}");

        #[cfg(any(
            all(
                any(feature = "wayland", feature = "x11"),
                any(
                    target_os = "linux",
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd",
                )
            ),
            target_os = "windows"
        ))]
        if let Some(name) = &window.name {
            #[cfg(all(
                feature = "wayland",
                any(
                    target_os = "linux",
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                )
            ))]
            {
                winit_window_attributes =
                    winit::platform::wayland::WindowAttributesExtWayland::with_name(
                        winit_window_attributes,
                        name.clone(),
                        "",
                    );
            }

            #[cfg(all(
                feature = "x11",
                any(
                    target_os = "linux",
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                )
            ))]
            {
                winit_window_attributes = winit::platform::x11::WindowAttributesExtX11::with_name(
                    winit_window_attributes,
                    name.clone(),
                    "",
                );
            }
            // `WindowAttributesWindows::with_class_name` is only available on Win32.
            // WinRT/UWP does not have window classes, and Bevy doesn't currently need this.
        }

        let constraints = window.resize_constraints.check_constraints();
        let min_inner_size = LogicalSize {
            width: constraints.min_width,
            height: constraints.min_height,
        };
        let max_inner_size = LogicalSize {
            width: constraints.max_width,
            height: constraints.max_height,
        };

        let winit_window_attributes =
            if constraints.max_width.is_finite() && constraints.max_height.is_finite() {
                winit_window_attributes
                    .with_min_surface_size(min_inner_size)
                    .with_max_surface_size(max_inner_size)
            } else {
                winit_window_attributes.with_min_surface_size(min_inner_size)
            };

        #[expect(clippy::allow_attributes, reason = "`unused_mut` is not always linted")]
        #[allow(
            unused_mut,
            reason = "This variable needs to be mutable if `cfg(target_arch = \"wasm32\")`"
        )]
        let mut winit_window_attributes = winit_window_attributes.with_title(window.title.as_str());

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;

            if let Some(selector) = &window.canvas {
                let window = web_sys::window().unwrap();
                let document = window.document().unwrap();
                let canvas = document
                    .query_selector(selector)
                    .expect("Cannot query for canvas element.");
                if let Some(canvas) = canvas {
                    let canvas = canvas.dyn_into::<web_sys::HtmlCanvasElement>().ok();
                    winit_window_attributes = winit_window_attributes.with_canvas(canvas);
                } else {
                    panic!("Cannot find element: {selector}.");
                }
            }

            winit_window_attributes =
                winit_window_attributes.with_prevent_default(window.prevent_default_event_handling);
            winit_window_attributes = winit_window_attributes.with_append(true);
        }

        let winit_window = event_loop.create_window(winit_window_attributes)?;
        winit_window.set_visible(window.visible);

        // Do not set the grab mode on window creation if it's none. It can fail on mobile.
        if cursor_options.grab_mode != CursorGrabMode::None {
            // Cursor grab is unsupported on some platforms, including WinRT/UWP.
            // Failures are expected and should not crash window creation.
            let _ = attempt_grab(winit_window.as_ref(), cursor_options.grab_mode);
        }

        winit_window.set_cursor_visible(cursor_options.visible);

        // Do not set the cursor hittest on window creation if it's false, as it will always fail on
        // some platforms and log an unfixable warning.
        if !cursor_options.hit_test {
            if let Err(err) = winit_window.set_cursor_hittest(cursor_options.hit_test) {
                #[cfg(all(target_os = "windows", __WINRT__))]
                if matches!(err, RequestError::NotSupported(_)) {
                    // Expected on WinRT/UWP.
                } else {
                    warn!(
                        "Could not set cursor hit test for window {}: {}",
                        window.title, err
                    );
                }
                #[cfg(not(all(target_os = "windows", __WINRT__)))]
                warn!(
                    "Could not set cursor hit test for window {}: {}",
                    window.title, err
                );
            }
        }

        self.entity_to_winit.insert(entity, winit_window.id());
        self.winit_to_entity.insert(winit_window.id(), entity);

        Ok(self
            .windows
            .entry(winit_window.id())
            .insert(WindowWrapper::new(winit_window))
            .into_mut())
    }

    /// Get the winit window that is associated with our entity.
    pub fn get_window(&self, entity: Entity) -> Option<&WindowWrapper<Box<dyn WinitWindow>>> {
        self.entity_to_winit
            .get(&entity)
            .and_then(|winit_id| self.windows.get(winit_id))
    }

    /// Get the entity associated with the winit window id.
    ///
    /// This is mostly just an intermediary step between us and winit.
    pub fn get_window_entity(&self, winit_id: WindowId) -> Option<Entity> {
        self.winit_to_entity.get(&winit_id).cloned()
    }

    /// Remove a window from winit.
    ///
    /// This should mostly just be called when the window is closing.
    pub fn remove_window(&mut self, entity: Entity) -> Option<WindowWrapper<Box<dyn WinitWindow>>> {
        let winit_id = self.entity_to_winit.remove(&entity)?;
        self.winit_to_entity.remove(&winit_id);
        self.windows.remove(&winit_id)
    }
}

/// Returns some [`winit::monitor::VideoMode`] given a [`MonitorHandle`] and a
/// [`VideoModeSelection`] or None if no valid matching video mode was found.
pub fn get_selected_videomode(
    monitor: &MonitorHandle,
    selection: &VideoModeSelection,
) -> Option<VideoMode> {
    match selection {
        VideoModeSelection::Current => monitor.current_video_mode(),
        VideoModeSelection::Specific(specified) => monitor.video_modes().find(|mode| {
            mode.size().width == specified.physical_size.x
                && mode.size().height == specified.physical_size.y
                && mode
                    .refresh_rate_millihertz()
                    .map(|rate| rate.get())
                    .unwrap_or(0)
                    == specified.refresh_rate_millihertz
                && mode.bit_depth().map(|depth| depth.get()).unwrap_or(0) == specified.bit_depth
        }),
    }
}

#[cfg(target_arch = "wasm32")]
fn pointer_supported() -> Result<bool, RequestError> {
    Ok(js_sys::Reflect::has(
        web_sys::window()
            .ok_or(RequestError::Ignored)?
            .document()
            .ok_or(RequestError::Ignored)?
            .as_ref(),
        &"exitPointerLock".into(),
    )
    .unwrap_or(false))
}

pub(crate) fn attempt_grab(
    winit_window: &dyn WinitWindow,
    grab_mode: CursorGrabMode,
) -> Result<(), RequestError> {
    // Do not attempt to grab on web if unsupported (e.g. mobile)
    #[cfg(target_arch = "wasm32")]
    if !pointer_supported()? {
        return Err(RequestError::Ignored);
    }

    let grab_result = match grab_mode {
        CursorGrabMode::None => winit_window.set_cursor_grab(WinitCursorGrabMode::None),
        CursorGrabMode::Confined => winit_window
            .set_cursor_grab(WinitCursorGrabMode::Confined)
            .or_else(|_e| winit_window.set_cursor_grab(WinitCursorGrabMode::Locked)),
        CursorGrabMode::Locked => winit_window
            .set_cursor_grab(WinitCursorGrabMode::Locked)
            .or_else(|_e| winit_window.set_cursor_grab(WinitCursorGrabMode::Confined)),
    };

    match grab_result {
        Ok(()) => Ok(()),
        Err(err) => {
            #[cfg(all(target_os = "windows", __WINRT__))]
            if matches!(err, RequestError::NotSupported(_)) {
                // Expected no-op / NotSupported on WinRT/UWP.
                return Ok(());
            }

            Err(err)
        }
    }
}

/// Compute the physical window position for a given [`WindowPosition`].
// Ideally we could generify this across window backends, but we only really have winit atm
// so whatever.
pub fn winit_window_position(
    position: &WindowPosition,
    resolution: &WindowResolution,
    monitors: &WinitMonitors,
    primary_monitor: Option<MonitorHandle>,
    current_monitor: Option<MonitorHandle>,
) -> Option<PhysicalPosition<i32>> {
    match position {
        WindowPosition::Automatic => {
            // Window manager will handle position
            None
        }
        WindowPosition::Centered(monitor_selection) => {
            let maybe_monitor = select_monitor(
                monitors,
                primary_monitor,
                current_monitor,
                monitor_selection,
            );

            if let Some(monitor) = maybe_monitor {
                let Some(video_mode) = monitor.current_video_mode() else {
                    warn!("Couldn't query current video mode for selected monitor");
                    return None;
                };
                let screen_size = video_mode.size();
                let monitor_pos = monitor.position().unwrap_or_else(|| (0, 0).into());

                let scale_factor = match resolution.scale_factor_override() {
                    Some(scale_factor_override) => scale_factor_override as f64,
                    // We use the monitors scale factor here since `WindowResolution.scale_factor` is
                    // not yet populated when windows are created during plugin setup.
                    None => monitor.scale_factor(),
                };

                // Logical to physical window size
                let (width, height): (u32, u32) =
                    LogicalSize::new(resolution.width(), resolution.height())
                        .to_physical::<u32>(scale_factor)
                        .into();

                let position = PhysicalPosition {
                    x: screen_size.width.saturating_sub(width) as f64 / 2.
                        + monitor_pos.x as f64,
                    y: screen_size.height.saturating_sub(height) as f64 / 2.
                        + monitor_pos.y as f64,
                };

                Some(position.cast::<i32>())
            } else {
                warn!("Couldn't get monitor selected with: {monitor_selection:?}");
                None
            }
        }
        WindowPosition::At(position) => {
            Some(PhysicalPosition::new(position[0] as f64, position[1] as f64).cast::<i32>())
        }
    }
}

/// Selects a monitor based on the given [`MonitorSelection`].
pub fn select_monitor(
    monitors: &WinitMonitors,
    primary_monitor: Option<MonitorHandle>,
    current_monitor: Option<MonitorHandle>,
    monitor_selection: &MonitorSelection,
) -> Option<MonitorHandle> {
    use bevy_window::MonitorSelection::*;

    match monitor_selection {
        Current => {
            if current_monitor.is_none() {
                warn!("Can't select current monitor on window creation or cannot find current monitor!");
            }
            current_monitor
        }
        Primary => primary_monitor,
        Index(n) => monitors.nth(*n),
        Entity(entity) => monitors.find_entity(*entity),
    }
}

struct DisplayInfo {
    window_physical_resolution: (u32, u32),
    window_logical_resolution: (f32, f32),
    monitor_name: Option<String>,
    scale_factor: Option<f64>,
    refresh_rate_millihertz: Option<u32>,
}

impl core::fmt::Display for DisplayInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Display information:")?;
        write!(
            f,
            "  Window physical resolution: {}x{}",
            self.window_physical_resolution.0, self.window_physical_resolution.1
        )?;
        write!(
            f,
            "  Window logical resolution: {}x{}",
            self.window_logical_resolution.0, self.window_logical_resolution.1
        )?;
        write!(
            f,
            "  Monitor name: {}",
            self.monitor_name.as_deref().unwrap_or("")
        )?;
        write!(f, "  Scale factor: {}", self.scale_factor.unwrap_or(0.))?;
        let millihertz = self.refresh_rate_millihertz.unwrap_or(0);
        let hertz = millihertz / 1000;
        let extra_millihertz = millihertz % 1000;
        write!(f, "  Refresh rate (Hz): {hertz}.{extra_millihertz:03}")?;
        Ok(())
    }
}
