//! Load and save. One JSON file, written atomically.

pub mod persist;
pub mod settings;

pub use persist::{DataStore, LoadOutcome, SCHEMA_VERSION, StoreError};
pub use settings::Settings;
