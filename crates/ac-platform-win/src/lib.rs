//! Windows side of AutoCorrect: the keyboard hook, key injection, focus and
//! password tracking, the tray icon. `autocorrect.exe` is a thin shell around
//! this library, and `ac-e2e` drives the same hook in a test window.

pub mod focus;
pub mod hook;
pub mod inject;
pub mod log;
pub mod policy;
pub mod settings;
pub mod tray;
