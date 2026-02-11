#![no_main]

use std::f32::consts::PI;

#[cfg(target_os = "windows")]
use std::any::Any;
#[cfg(all(target_os = "windows", __WINRT__))]
use std::panic::PanicHookInfo;
#[cfg(target_os = "windows")]
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
#[cfg(all(target_os = "windows", __WINRT__))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(all(target_os = "windows", __WINRT__))]
use std::sync::Arc;

#[cfg(all(target_os = "windows", __WINRT__))]
use bevy::app::{TaskPoolOptions, TaskPoolPlugin};
use bevy::asset::RenderAssetUsages;
use bevy::asset::{AssetMetaCheck, AssetMode, AssetPlugin, UnapprovedPathMode};
use bevy::color::palettes::basic::SILVER;
use bevy::input::common_conditions::input_toggle_active;
use bevy::input::keyboard::Key;
#[cfg(not(target_arch = "wasm32"))]
use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::prelude::*;
#[cfg(all(target_os = "windows", __WINRT__))]
use bevy::render::error_handler::{RenderError, RenderErrorHandler, RenderErrorPolicy};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
#[cfg(all(target_os = "windows", __WINRT__))]
use bevy::render::settings::{
    Backends, Dx12Compiler, Gles3MinorVersion, PowerPreference, WgpuSettings, WgpuSettingsPriority,
};
use bevy::render::RenderPlugin;
#[cfg(all(target_os = "windows", __WINRT__))]
use windows::ApplicationModel::Package;
#[cfg(all(target_os = "windows", __WINRT__))]
use windows::Storage::ApplicationData;
#[cfg(all(target_os = "windows", __WINRT__))]
use windows::System::Profile::AnalyticsInfo;
#[cfg(target_os = "windows")]
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

#[cfg(all(target_os = "windows", __WINRT__))]
static PANIC_DIALOG_SHOWN: AtomicBool = AtomicBool::new(false);
#[cfg(all(target_os = "windows", __WINRT__))]
static RENDER_ERROR_DIALOG_SHOWN: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "windows")]
fn panic_payload_to_string(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "panic payload is not a string".to_owned()
    }
}

#[cfg(all(target_os = "windows", __WINRT__))]
fn panic_info_to_string(info: &PanicHookInfo<'_>) -> String {
    let message = panic_payload_to_string(info.payload());
    if let Some(location) = info.location() {
        format!(
            "{message}\n\nat {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        )
    } else {
        message
    }
}

#[cfg(all(target_os = "windows", __WINRT__))]
fn show_winrt_crash_dialog(title: &str, body: &str) {
    use windows::ApplicationModel::Core::CoreApplication;
    use windows::UI::Core::{CoreDispatcherPriority, CoreWindow, DispatchedHandler};
    use windows::UI::Popups::MessageDialog;

    if PANIC_DIALOG_SHOWN.swap(true, Ordering::SeqCst) {
        return;
    }

    if CoreWindow::GetForCurrentThread().is_ok() {
        let body_h = windows::core::HSTRING::from(body);
        let title_h = windows::core::HSTRING::from(title);
        if let Ok(dialog) = MessageDialog::CreateWithTitle(&body_h, &title_h) {
            let _ = dialog.ShowAsync();
        }
        return;
    }

    let title_owned = title.to_owned();
    let body_owned = body.to_owned();
    if let Ok(view) = CoreApplication::MainView() {
        if let Ok(window) = view.CoreWindow() {
            if let Ok(dispatcher) = window.Dispatcher() {
                let action = dispatcher.RunAsync(
                    CoreDispatcherPriority::High,
                    &DispatchedHandler::new(move || {
                        let body_h = windows::core::HSTRING::from(body_owned.as_str());
                        let title_h = windows::core::HSTRING::from(title_owned.as_str());
                        if let Ok(dialog) = MessageDialog::CreateWithTitle(&body_h, &title_h) {
                            let _ = dialog.ShowAsync();
                        }
                        Ok(())
                    }),
                );
                let _ = action;
            }
        }
    }
}

#[cfg(all(target_os = "windows", __WINRT__))]
fn install_winrt_panic_dialog_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let panic_text = panic_info_to_string(info);
        show_winrt_crash_dialog("winit_test panic", &panic_text);
        default_hook(info);
    }));
}

#[cfg(all(target_os = "windows", __WINRT__))]
fn winrt_task_pool_plugin() -> TaskPoolPlugin {
    let init: Arc<dyn Fn() + Send + Sync + 'static> = Arc::new(|| {
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    });

    let mut options = TaskPoolOptions::default();
    options.io.on_thread_spawn = Some(init.clone());
    options.async_compute.on_thread_spawn = Some(init.clone());
    options.compute.on_thread_spawn = Some(init.clone());

    TaskPoolPlugin {
        task_pool_options: options,
    }
}

#[cfg(all(target_os = "windows", __WINRT__))]
fn winrt_render_error_policy(
    error: &RenderError,
    _main_world: &mut bevy::ecs::world::World,
    _render_world: &mut bevy::ecs::world::World,
) -> RenderErrorPolicy {
    if !RENDER_ERROR_DIALOG_SHOWN.swap(true, Ordering::SeqCst) {
        let msg = format!("type: {:?}\n\n{}", error.ty, error.description);
        show_winrt_crash_dialog("bevy render error", &msg);
    }
    RenderErrorPolicy::StopRendering
}

#[cfg(all(target_os = "windows", __WINRT__))]
fn winrt_local_state_assets_path() -> Option<PathBuf> {
    ApplicationData::Current()
        .ok()
        .and_then(|data| data.LocalFolder().ok())
        .and_then(|folder| folder.Path().ok())
        .map(|path| PathBuf::from(path.to_os_string()).join("assets"))
}

#[cfg(all(target_os = "windows", __WINRT__))]
fn winrt_packaged_assets_path() -> Option<PathBuf> {
    Package::Current()
        .ok()
        .and_then(|package| package.InstalledLocation().ok())
        .and_then(|folder| folder.Path().ok())
        .map(|path| PathBuf::from(path.to_os_string()).join("assets"))
}

fn uwp_asset_plugin() -> AssetPlugin {
    #[cfg(all(target_os = "windows", __WINRT__))]
    let assets_path = {
        let local_state_assets = winrt_local_state_assets_path();
        let packaged_assets = winrt_packaged_assets_path();

        if let Some(local) = local_state_assets {
            if std::fs::create_dir_all(&local).is_ok() || local.exists() {
                local
            } else if let Some(packaged) = packaged_assets {
                if packaged.exists() {
                    packaged
                } else {
                    panic!(
                        "WinRT asset root unavailable: LocalState/assets not writable and packaged assets missing"
                    )
                }
            } else {
                panic!(
                    "WinRT asset root unavailable: LocalState/assets not writable and package location unavailable"
                )
            }
        } else {
            packaged_assets
                .filter(|packaged| packaged.exists())
                .expect(
                    "WinRT asset root unavailable: LocalState not accessible and packaged assets missing",
                )
        }
    };

    #[cfg(not(all(target_os = "windows", __WINRT__)))]
    let assets_path = PathBuf::from(".").join("assets");

    let assets_path = assets_path.to_string_lossy().into_owned();
    AssetPlugin {
        file_path: assets_path.clone(),
        processed_file_path: assets_path,
        watch_for_changes_override: Some(false),
        use_asset_processor_override: Some(false),
        mode: AssetMode::Unprocessed,
        meta_check: AssetMetaCheck::Never,
        unapproved_path_mode: UnapprovedPathMode::Forbid,
    }
}

fn render_plugin() -> RenderPlugin {
    #[cfg(all(target_os = "windows", __WINRT__))]
    {
        let mut settings = WgpuSettings::default();

        const FORCE_BACKEND: Option<Backends> = None;
        if let Some(backends) = FORCE_BACKEND {
            settings.backends = Some(backends);
        }

        settings.gles3_minor_version = Gles3MinorVersion::Version0;
        settings.power_preference = PowerPreference::HighPerformance;
        settings.priority = WgpuSettingsPriority::Functionality;

        settings.dx12_shader_compiler = Dx12Compiler::Fxc;

        RenderPlugin {
            render_creation: settings.into(),
            synchronous_pipeline_compilation: true,
            ..default()
        }
    }

    #[cfg(not(all(target_os = "windows", __WINRT__)))]
    {
        RenderPlugin::default()
    }
}

#[derive(Component)]
struct Shape;

const SHAPES_X_EXTENT: f32 = 14.0;
const EXTRUSION_X_EXTENT: f32 = 14.0;
const Z_EXTENT: f32 = 8.0;
const THICKNESS: f32 = 0.1;

#[derive(Resource, Default)]
struct OverlayStats {
    fps: f32,
}

#[derive(Resource)]
struct PlatformName(String);

#[derive(Component)]
struct OverlayPanelText;

fn detect_platform_name() -> String {
    #[cfg(all(target_os = "windows", __WINRT__))]
    {
        let family = AnalyticsInfo::VersionInfo()
            .ok()
            .and_then(|info| info.DeviceFamily().ok())
            .map(|name| name.to_string());
        return match family.as_deref() {
            Some(name) => name.to_ascii_lowercase(),
            None => "windows.unknown".to_string(),
        };
    }

    #[cfg(not(all(target_os = "windows", __WINRT__)))]
    {
        format!("{}.{}", std::env::consts::OS, std::env::consts::ARCH)
    }
}

fn update_overlay_stats(time: Res<Time>, mut stats: ResMut<OverlayStats>) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let instant_fps = 1.0 / dt;
    stats.fps = if stats.fps <= 0.0 {
        instant_fps
    } else {
        stats.fps * 0.9 + instant_fps * 0.1
    };
}

fn update_overlay_panel_text(
    mut query: Query<&mut Text, With<OverlayPanelText>>,
    stats: Res<OverlayStats>,
    platform: Res<PlatformName>,
    adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>,
) {
    let renderer = if let Some(adapter) = adapter {
        format!("{:?} | {}", adapter.0.backend, adapter.0.name)
    } else {
        "initializing".to_string()
    };

    let value = format!(
        "FPS: {:.1}\nRenderer: {renderer}\nPlatform: {}",
        stats.fps, platform.0
    );

    for mut text in &mut query {
        text.0 = value.clone();
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let debug_material = materials.add(StandardMaterial {
        base_color_texture: Some(images.add(uv_debug_texture())),
        ..default()
    });

    let shapes = [
        meshes.add(Cuboid::default()),
        meshes.add(Tetrahedron::default()),
        meshes.add(Capsule3d::default()),
        meshes.add(Torus::default()),
        meshes.add(Cylinder::default()),
        meshes.add(Cone::default()),
        meshes.add(ConicalFrustum::default()),
        meshes.add(Sphere::default().mesh().ico(5).unwrap()),
        meshes.add(Sphere::default().mesh().uv(32, 18)),
        meshes.add(Segment3d::default()),
        meshes.add(Polyline3d::new(vec![
            Vec3::new(-0.5, 0.0, 0.0),
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(0.0, 0.5, 0.0),
        ])),
    ];

    let extrusions = [
        meshes.add(Extrusion::new(Rectangle::default(), 1.)),
        meshes.add(Extrusion::new(Capsule2d::default(), 1.)),
        meshes.add(Extrusion::new(Annulus::default(), 1.)),
        meshes.add(Extrusion::new(Circle::default(), 1.)),
        meshes.add(Extrusion::new(Ellipse::default(), 1.)),
        meshes.add(Extrusion::new(RegularPolygon::default(), 1.)),
        meshes.add(Extrusion::new(Triangle2d::default(), 1.)),
    ];

    let ring_extrusions = [
        meshes.add(Extrusion::new(Rectangle::default().to_ring(THICKNESS), 1.)),
        meshes.add(Extrusion::new(Capsule2d::default().to_ring(THICKNESS), 1.)),
        meshes.add(Extrusion::new(
            Ring::new(Circle::new(1.0), Circle::new(0.5)),
            1.,
        )),
        meshes.add(Extrusion::new(Circle::default().to_ring(THICKNESS), 1.)),
        meshes.add(Extrusion::new(
            {
                let outer = Ellipse::default();
                let mut inner = outer;
                inner.half_size -= Vec2::splat(THICKNESS);
                Ring::new(outer, inner)
            },
            1.,
        )),
        meshes.add(Extrusion::new(
            RegularPolygon::default().to_ring(THICKNESS),
            1.,
        )),
        meshes.add(Extrusion::new(Triangle2d::default().to_ring(THICKNESS), 1.)),
    ];

    let num_shapes = shapes.len();
    for (i, shape) in shapes.into_iter().enumerate() {
        commands.spawn((
            Mesh3d(shape),
            MeshMaterial3d(debug_material.clone()),
            Transform::from_xyz(
                -SHAPES_X_EXTENT / 2. + i as f32 / (num_shapes - 1) as f32 * SHAPES_X_EXTENT,
                2.0,
                Row::Front.z(),
            )
            .with_rotation(Quat::from_rotation_x(-PI / 4.)),
            Shape,
            Row::Front,
        ));
    }

    let num_extrusions = extrusions.len();
    for (i, shape) in extrusions.into_iter().enumerate() {
        commands.spawn((
            Mesh3d(shape),
            MeshMaterial3d(debug_material.clone()),
            Transform::from_xyz(
                -EXTRUSION_X_EXTENT / 2.
                    + i as f32 / (num_extrusions - 1) as f32 * EXTRUSION_X_EXTENT,
                2.0,
                Row::Middle.z(),
            )
            .with_rotation(Quat::from_rotation_x(-PI / 4.)),
            Shape,
            Row::Middle,
        ));
    }

    let num_ring_extrusions = ring_extrusions.len();
    for (i, shape) in ring_extrusions.into_iter().enumerate() {
        commands.spawn((
            Mesh3d(shape),
            MeshMaterial3d(debug_material.clone()),
            Transform::from_xyz(
                -EXTRUSION_X_EXTENT / 2.
                    + i as f32 / (num_ring_extrusions - 1) as f32 * EXTRUSION_X_EXTENT,
                2.0,
                Row::Rear.z(),
            )
            .with_rotation(Quat::from_rotation_x(-PI / 4.)),
            Shape,
            Row::Rear,
        ));
    }

    commands.spawn((
        PointLight {
            intensity: 10_000_000.,
            range: 100.0,
            shadow_maps_enabled: true,
            shadow_depth_bias: 0.2,
            ..default()
        },
        Transform::from_xyz(8.0, 16.0, 8.0),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(50.0, 50.0).subdivisions(10))),
        MeshMaterial3d(materials.add(Color::from(SILVER))),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 7., 14.0).looking_at(Vec3::new(0., 1., 0.), Vec3::Y),
    ));

    let mut text = "\
Press 'R' to pause/resume rotation\n\
Press 'Tab' (or Xbox 'B') to cycle through rows"
        .to_string();
    #[cfg(not(target_arch = "wasm32"))]
    text.push_str("\nPress 'Space' to toggle wireframes");

    commands.spawn((
        Text::new(text),
        Node {
            position_type: PositionType::Absolute,
            top: bevy::ui::Val::Px(12.0),
            left: bevy::ui::Val::Px(12.0),
            ..default()
        },
    ));

    commands.spawn((
        Text::new("FPS: --\nRenderer: initializing\nPlatform: initializing"),
        Node {
            position_type: PositionType::Absolute,
            top: bevy::ui::Val::Px(12.0),
            right: bevy::ui::Val::Px(12.0),
            ..default()
        },
        OverlayPanelText,
    ));
}

fn rotate(mut query: Query<&mut Transform, With<Shape>>, time: Res<Time>) {
    for mut transform in &mut query {
        transform.rotate_y(time.delta_secs() / 2.);
    }
}

fn uv_debug_texture() -> Image {
    const TEXTURE_SIZE: usize = 8;

    let mut palette: [u8; 32] = [
        255, 102, 159, 255, 255, 159, 102, 255, 236, 255, 102, 255, 121, 255, 102, 255, 102, 255,
        198, 255, 102, 198, 255, 255, 121, 102, 255, 255, 236, 102, 255, 255,
    ];

    let mut texture_data = [0; TEXTURE_SIZE * TEXTURE_SIZE * 4];
    for y in 0..TEXTURE_SIZE {
        let offset = TEXTURE_SIZE * y * 4;
        texture_data[offset..(offset + TEXTURE_SIZE * 4)].copy_from_slice(&palette);
        palette.rotate_right(4);
    }

    Image::new_fill(
        Extent3d {
            width: TEXTURE_SIZE as u32,
            height: TEXTURE_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &texture_data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn toggle_wireframe(
    mut wireframe_config: ResMut<WireframeConfig>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if keyboard.just_pressed(KeyCode::Space) {
        wireframe_config.global = !wireframe_config.global;
    }
}

#[derive(Component, Clone, Copy)]
enum Row {
    Front,
    Middle,
    Rear,
}

impl Row {
    fn z(self) -> f32 {
        match self {
            Row::Front => Z_EXTENT / 2.,
            Row::Middle => 0.,
            Row::Rear => -Z_EXTENT / 2.,
        }
    }

    fn advance(self) -> Self {
        match self {
            Row::Front => Row::Rear,
            Row::Middle => Row::Front,
            Row::Rear => Row::Middle,
        }
    }
}

fn advance_rows(mut shapes: Query<(&mut Row, &mut Transform), With<Shape>>) {
    for (mut row, mut transform) in &mut shapes {
        *row = row.advance();
        transform.translation.z = row.z();
    }
}

fn tab_or_back_just_pressed(keys: Res<ButtonInput<Key>>) -> bool {
    keys.just_pressed(Key::Tab) || keys.just_pressed(Key::GoBack)
}

fn run_bevy_app() {
    #[cfg(all(target_os = "windows", __WINRT__))]
    let plugins = DefaultPlugins
        .build()
        .set(winrt_task_pool_plugin())
        .set(bevy::window::WindowPlugin {
            primary_window: Some(bevy::window::Window {
                title: "winit_test_bevy (WinRT)".to_string(),
                ..default()
            }),
            ..default()
        })
        .set(ImagePlugin::default_nearest())
        .set(uwp_asset_plugin())
        .set(render_plugin());

    #[cfg(not(all(target_os = "windows", __WINRT__)))]
    let plugins = DefaultPlugins
        .build()
        .disable::<bevy::app::TerminalCtrlCHandlerPlugin>()
        .set(ImagePlugin::default_nearest())
        .set(uwp_asset_plugin())
        .set(render_plugin());

    let mut app = App::new();
    app.add_plugins(plugins);
    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(WireframePlugin::default());
    app.insert_resource(OverlayStats::default());
    app.insert_resource(PlatformName(detect_platform_name()));

    #[cfg(all(target_os = "windows", __WINRT__))]
    app.insert_resource(RenderErrorHandler(winrt_render_error_policy));

    app.add_systems(Startup, setup).add_systems(
        Update,
        (
            rotate.run_if(input_toggle_active(true, KeyCode::KeyR)),
            advance_rows.run_if(tab_or_back_just_pressed),
            update_overlay_stats,
            update_overlay_panel_text,
            #[cfg(not(target_arch = "wasm32"))]
            toggle_wireframe,
        ),
    );

    app.run();
}

#[no_mangle]
pub extern "system" fn wWinMain(
    _instance: isize,
    _prev_instance: isize,
    _cmd_line: *mut u16,
    _show_cmd: i32,
) -> i32 {
    #[cfg(target_os = "windows")]
    {
        #[cfg(all(target_os = "windows", __WINRT__))]
        {
            let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        }

        #[cfg(not(all(target_os = "windows", __WINRT__)))]
        {
            let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        }
    }

    #[cfg(target_os = "windows")]
    {
        #[cfg(all(target_os = "windows", __WINRT__))]
        install_winrt_panic_dialog_hook();

        let result = panic::catch_unwind(AssertUnwindSafe(run_bevy_app));
        if let Err(payload) = result {
            let panic_text = panic_payload_to_string(payload.as_ref());

            #[cfg(all(target_os = "windows", __WINRT__))]
            show_winrt_crash_dialog("winit_test crashed", &panic_text);
            #[cfg(not(all(target_os = "windows", __WINRT__)))]
            eprintln!("winit_test crashed: {panic_text}");

            return 1;
        }
        return 0;
    }

    #[cfg(not(all(target_os = "windows", __WINRT__)))]
    {
        run_bevy_app();
        0
    }
}
