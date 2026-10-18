//! Tauri commands exposed to the frontend.

use crate::models::EnvironmentStatus;

/// Report which external binaries were found and their versions.
#[tauri::command]
pub fn check_environment() -> Result<EnvironmentStatus, String> {
    crate::ytdlp::environment_status()
}
