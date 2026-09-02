//! Interface preferences.
//!
//! Kept in a sibling file rather than inside `data.json`, so a preference can
//! never put the task data at risk, and a corrupt preference file costs nothing
//! more than a reset zoom.
//!
//! This module deliberately depends on `ui` for [`Panel`]: it stores UI
//! preferences, not core data. The data path in `persist` depends only on the
//! model, which is the boundary that actually matters.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ui::Panel;

/// Bounds on the zoom factor. Outside these the interface is unusable.
pub const ZOOM_MIN: f32 = 0.7;
pub const ZOOM_MAX: f32 = 2.5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Interface scale, applied to every text size and spacing value.
    pub zoom: f32,
    /// Which side panel was showing when the app last closed.
    pub panel: Panel,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            panel: Panel::None,
        }
    }
}

impl Settings {
    /// The preferences file that sits beside a given data file.
    #[must_use]
    pub fn path_beside(data: &Path) -> PathBuf {
        data.with_file_name("settings.json")
    }

    /// Never fails. Anything unreadable or out of range falls back to defaults.
    #[must_use]
    pub fn load(data: &Path) -> Self {
        let path = Self::path_beside(data);
        let mut settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .unwrap_or_default();
        if !settings.zoom.is_finite() {
            settings.zoom = 1.0;
        }
        settings.zoom = settings.zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        settings
    }

    /// Best effort. A preference that fails to save is not worth interrupting
    /// anyone over, so it is logged and dropped.
    pub fn save(&self, data: &Path) {
        let path = Self::path_beside(data);
        if let Some(parent) = path.parent()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            log::warn!("could not create {}: {err}", parent.display());
            return;
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(err) = std::fs::write(&path, json) {
                    log::warn!("could not save settings to {}: {err}", path.display());
                }
            }
            Err(err) => log::warn!("could not encode settings: {err}"),
        }
    }
}
