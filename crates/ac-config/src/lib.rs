//! Everything the user can configure, and where it is stored. Shared by the
//! background app and the settings window; knows nothing about the OS beyond
//! where the config folder is.

pub mod apps;
pub mod hotkey;
pub mod paths;
pub mod personal;
pub mod settings;

pub use apps::{AppKind, Apps};
pub use hotkey::{Detector, Hotkey, Outcome};
pub use settings::{Settings, Strength};
