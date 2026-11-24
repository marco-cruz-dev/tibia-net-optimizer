//! egui window + system tray glue (feature `gui`).
//!
//! Deliberately thin: everything it shows comes from [`crate::app::view`],
//! and everything the user does becomes a [`crate::app::state::Msg`].

mod tray;
mod window;

pub use window::run;
