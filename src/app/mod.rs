//! GUI application logic, independent of any UI toolkit.
//!
//! - [`state`]: messages in → state changes + effects out (pure reducer)
//! - [`view`]: what the window and tray should show (pure)
//! - [`icon`]: status icon pixels (pure)
//! - [`worker`]: background thread that talks to the OS
//!
//! The egui/tray glue in `crate::ui` only renders [`view::View`] and turns
//! clicks into [`state::Msg`]s.

pub mod icon;
pub mod ping;
pub mod state;
pub mod view;
pub mod worker;
