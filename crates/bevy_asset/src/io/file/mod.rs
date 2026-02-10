#[cfg(feature = "file_watcher")]
mod file_watcher;

#[cfg(feature = "multi_threaded")]
mod file_asset;
#[cfg(not(feature = "multi_threaded"))]
mod sync_file_asset;

#[cfg(feature = "file_watcher")]
pub use file_watcher::*;
use tracing::{debug, error};

#[cfg(not(all(target_os = "windows", __WINRT__)))]
use alloc::borrow::ToOwned;
use std::{
    env,
    path::{Path, PathBuf},
};

pub(crate) fn get_base_path() -> PathBuf {
    // WinRT/UWP apps are sandboxed. Relative paths should resolve under LocalState by default.
    #[cfg(all(target_os = "windows", __WINRT__))]
    {
        if let Ok(root) = env::var("BEVY_ASSET_ROOT") {
            let root = PathBuf::from(root);
            if root.is_absolute() {
                root
            } else {
                winrt_local_state_dir().join(root)
            }
        } else {
            winrt_local_state_dir()
        }
    }

    #[cfg(not(all(target_os = "windows", __WINRT__)))]
    {
        if let Ok(manifest_dir) = env::var("BEVY_ASSET_ROOT") {
            PathBuf::from(manifest_dir)
        } else if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
            PathBuf::from(manifest_dir)
        } else {
            env::current_exe()
                .map(|path| path.parent().map(ToOwned::to_owned).unwrap())
                .unwrap()
        }
    }
}

/// I/O implementation for the local filesystem.
///
/// This asset I/O is fully featured but it's not available on `android` and `wasm` targets.
pub struct FileAssetReader {
    root_path: PathBuf,
    #[cfg(all(target_os = "windows", __WINRT__))]
    fallback_root_path: Option<PathBuf>,
}

impl FileAssetReader {
    /// Creates a new `FileAssetIo` at a path relative to the executable's directory, optionally
    /// watching for changes.
    ///
    /// See `get_base_path` below.
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let root_path = Self::get_base_path().join(path.as_ref());
        #[cfg(all(target_os = "windows", __WINRT__))]
        let fallback_root_path = winrt_installed_location_dir().map(|p| p.join(path.as_ref()));
        debug!(
            "Asset Server using {} as its base path.",
            root_path.display()
        );
        Self {
            root_path,
            #[cfg(all(target_os = "windows", __WINRT__))]
            fallback_root_path,
        }
    }

    /// Returns the base path of the assets directory, which is normally the executable's parent
    /// directory.
    ///
    /// To change this, set [`AssetPlugin::file_path`][crate::AssetPlugin::file_path].
    pub fn get_base_path() -> PathBuf {
        get_base_path()
    }

    /// Returns the root directory where assets are loaded from.
    ///
    /// See `get_base_path`.
    pub fn root_path(&self) -> &PathBuf {
        &self.root_path
    }

    #[cfg(all(target_os = "windows", __WINRT__))]
    pub(crate) fn fallback_root_path(&self) -> Option<&PathBuf> {
        self.fallback_root_path.as_ref()
    }
}

/// A writer for the local filesystem.
pub struct FileAssetWriter {
    root_path: PathBuf,
}

impl FileAssetWriter {
    /// Creates a new [`FileAssetWriter`] at a path relative to the executable's directory, optionally
    /// watching for changes.
    pub fn new<P: AsRef<Path> + core::fmt::Debug>(path: P, create_root: bool) -> Self {
        let root_path = get_base_path().join(path.as_ref());
        if create_root && let Err(e) = std::fs::create_dir_all(&root_path) {
            error!(
                "Failed to create root directory {} for file asset writer: {}",
                root_path.display(),
                e
            );
        }
        Self { root_path }
    }
}

#[cfg(all(target_os = "windows", __WINRT__))]
pub(crate) fn winrt_local_state_dir() -> PathBuf {
    use windows::Storage::ApplicationData;

    ApplicationData::Current()
        .ok()
        .and_then(|data| data.LocalFolder().ok())
        .and_then(|folder| folder.Path().ok())
        .map(|path| PathBuf::from(path.to_os_string()))
        // If WinRT APIs aren't available for some reason, fall back to a relative path.
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(all(target_os = "windows", __WINRT__))]
pub(crate) fn winrt_installed_location_dir() -> Option<PathBuf> {
    use windows::ApplicationModel::Package;

    Package::Current()
        .ok()
        .and_then(|package| package.InstalledLocation().ok())
        .and_then(|folder| folder.Path().ok())
        .map(|path| PathBuf::from(path.to_os_string()))
}
