//! System tray icon and menu.

use std::sync::mpsc::Sender;

use eframe::egui;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::actions::Action;
use crate::app::icon::{tray_icon_rgba, TRAY_SIZE};
use crate::app::state::Msg;
use crate::app::view::{Tone, TrayView};

pub struct Tray {
    icon: TrayIcon,
    status: MenuItem,
    shown: Option<TrayView>,
}

fn make_icon(tone: Tone) -> Result<Icon, String> {
    Icon::from_rgba(tray_icon_rgba(TRAY_SIZE, tone), TRAY_SIZE, TRAY_SIZE).map_err(|e| e.to_string())
}

impl Tray {
    /// Tray events are forwarded to `tx` and wake the UI via `ctx`.
    pub fn new(tx: Sender<Msg>, ctx: egui::Context) -> Result<Tray, String> {
        let status = MenuItem::new("Status: Checking...", false, None);
        let open = MenuItem::new("Open Dashboard", true, None);
        let apply = MenuItem::new("Apply Optimizations Now", true, None);
        let revert = MenuItem::new("Revert to Defaults", true, None);
        let quit = MenuItem::new("Quit", true, None);

        let menu = Menu::new();
        menu.append_items(&[
            &status,
            &open,
            &apply,
            &revert,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(|e| e.to_string())?;

        let ids = [
            (open.id().clone(), Msg::ShowDashboard),
            (apply.id().clone(), Msg::Request(Action::ApplyAll)),
            (revert.id().clone(), Msg::Request(Action::RevertAll)),
            (quit.id().clone(), Msg::Quit),
        ];
        let (menu_tx, menu_ctx) = (tx.clone(), ctx.clone());
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some((_, msg)) = ids.iter().find(|(id, _)| *id == event.id) {
                let _ = menu_tx.send(msg.clone());
                menu_ctx.request_repaint();
            }
        }));
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = tx.send(Msg::ShowDashboard);
                ctx.request_repaint();
            }
        }));

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("Tibia Network Optimizer")
            .with_icon(make_icon(Tone::Neutral)?)
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Tray {
            icon,
            status,
            shown: None,
        })
    }

    /// Updates icon colour, tooltip and the status item when they change.
    pub fn update(&mut self, view: &TrayView) {
        if self.shown.as_ref() == Some(view) {
            return;
        }
        if let Ok(icon) = make_icon(view.tone) {
            let _ = self.icon.set_icon(Some(icon));
        }
        let _ = self.icon.set_tooltip(Some(&view.tooltip));
        self.status.set_text(view.status_text);
        self.shown = Some(view.clone());
    }
}
