# winit_test_bevy (WinRT/UWP demo)

Minimal Bevy + WinRT demo that runs on any UWP device (Windows phone, Xbox, PC).

## What this demo does

- Starts Bevy from `wWinMain` in `src/main.rs`.
- Uses WinRT-compatible windowing/render stack (`winit` + `wgpu` WinRT forks).
- Renders a 3D shapes scene.
- Shows a top-right runtime overlay: FPS, current renderer backend/device, and platform (`windows.xbox`, `windows.pc`, etc.).

## Why there is no normal `main`

UWP apps are launched by the Windows runtime.  
This demo uses:

- `#![no_main]`
- `pub extern "system" fn wWinMain(...) -> i32`

`wWinMain` initializes WinRT (`RoInitialize`) and then runs the Bevy app.

## Dependencies used

- `bevy` from: `https://github.com/momo-AUX1/bevy.git` (`WinRT` branch)
- `winit` and `wgpu` patched to WinRT forks in `Cargo.toml`

Build this example from `bevy/` with `--manifest-path`.

### PowerShell (x64)

```powershell
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "x86_64-w64-mingw32-gcc"
$env:RUSTFLAGS = "--cfg __WINRT__"
cargo build --manifest-path .\examples\winrt\Cargo.toml --target x86_64-pc-windows-gnu
```

### Bash (x64)

```bash
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
export RUSTFLAGS="--cfg __WINRT__"
cargo build --manifest-path ./examples/winrt/Cargo.toml --target x86_64-pc-windows-gnu
```

## Packaging / registration

`build.rs` copies app assets and emits `AppxManifest.xml` into the build output directory (`target/<triple>/<profile>/`).

For local dev registration:

```powershell
Add-AppxPackage -Register .\target\x86_64-pc-windows-gnu\debug\AppxManifest.xml
```

## Optional backend override for testing

In `src/main.rs`, edit `FORCE_BACKEND` in `render_plugin()`:

- `None` => auto select
- `Some(Backends::DX12)` => DX12 only
- `Some(Backends::GL)` => GL only
