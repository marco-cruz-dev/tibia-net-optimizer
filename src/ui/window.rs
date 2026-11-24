//! The dashboard window (eframe/egui).

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};

use super::tray::Tray;
use crate::actions::Action;
use crate::app::icon::{circle_rgba, tone_rgb, GREY};
use crate::app::state::{AppState, Effect, Msg, NoticeKind};
use crate::app::view::{Tone, TrayView, View};
use crate::app::worker::Worker;
use crate::config::Config;
use crate::platform::{self, Os};
use crate::system::{elevation, notify};

const TITLE: &str = "Tibia Network Optimizer";

/// Opens the dashboard (hidden in the tray when `minimized`). Blocks until quit.
pub fn run(minimized: bool) -> Result<(), String> {
    let os = Os::current();
    let elevated = elevation::is_elevated();
    let config_path = Config::default_path();
    let config = config_path.as_deref().map(Config::load).unwrap_or_default();
    let (state, effects) = AppState::new(os, elevated, config, minimized);

    let icon = egui::IconData {
        rgba: circle_rgba(64, GREY),
        width: 64,
        height: 64,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size([420.0, 560.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_visible(!minimized)
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        options,
        Box::new(move |cc| Ok(Box::new(Dashboard::new(cc, state, effects, config_path)))),
    )
    .map_err(|e| e.to_string())
}

struct Dashboard {
    state: AppState,
    worker: Worker,
    tray: Option<Tray>,
    ui_events: Receiver<Msg>,
    config_path: Option<PathBuf>,
    allow_close: bool,
    /// eframe always shows the window after its first painted frame (egui
    /// PR #2279), so a `--minimized` start hides it right after that frame.
    pending_initial_hide: bool,
    painted: bool,
}

fn colour(tone: Tone) -> Color32 {
    let [r, g, b] = tone_rgb(tone);
    Color32::from_rgb(r, g, b)
}

impl Dashboard {
    fn new(
        cc: &eframe::CreationContext<'_>,
        state: AppState,
        effects: Vec<Effect>,
        config_path: Option<PathBuf>,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        let (tx, ui_events) = mpsc::channel();

        let wake = ctx.clone();
        // At least 10 s, so a hand-edited config can't make it spin.
        let interval = Duration::from_secs(state.config.check_interval_secs.max(10));
        let worker = Worker::spawn(
            platform::current,
            state.elevated,
            interval,
            Box::new(elevation::run_elevated_helper),
            move || wake.request_repaint(),
        );

        let tray = match Tray::new(tx, ctx.clone()) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("System tray unavailable: {e}");
                None
            }
        };

        let mut app = Dashboard {
            state,
            worker,
            tray,
            ui_events,
            config_path,
            allow_close: false,
            pending_initial_hide: false,
            painted: false,
        };
        if app.tray.is_none() {
            // Without a tray a hidden window could never come back.
            app.state.config.minimize_to_tray = false;
            if !app.state.window_visible {
                app.dispatch(&ctx, Msg::ShowDashboard);
            }
        }
        for e in effects {
            app.run_effect(&ctx, e);
        }
        app.pending_initial_hide = !app.state.window_visible;
        app
    }

    fn dispatch(&mut self, ctx: &egui::Context, msg: Msg) {
        for effect in self.state.update(msg) {
            self.run_effect(ctx, effect);
        }
    }

    fn run_effect(&mut self, ctx: &egui::Context, effect: Effect) {
        match effect {
            Effect::Worker(cmd) => self.worker.send(cmd),
            Effect::SaveConfig => {
                if let Some(path) = &self.config_path {
                    if let Err(e) = self.state.config.save(path) {
                        eprintln!("Could not save settings: {e}");
                    }
                }
            }
            Effect::ShowWindow => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            Effect::HideWindow => ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false)),
            Effect::RestartElevated => match elevation::relaunch_elevated(&[]) {
                Ok(()) => self.quit(ctx),
                Err(e) => self.dispatch(ctx, Msg::RestartFailed(e.to_string())),
            },
            Effect::Notify { title, body } => {
                // Off the UI thread: toasts and notify-send can be slow.
                std::thread::spawn(move || {
                    if let Err(e) = notify::send(&title, &body) {
                        eprintln!("Notification failed: {e}");
                    }
                });
            }
            Effect::Quit => self.quit(ctx),
        }
    }

    fn quit(&mut self, ctx: &egui::Context) {
        self.allow_close = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn render(&self, ui: &mut egui::Ui, view: &View) -> Vec<Msg> {
        let mut out = Vec::new();

        ui.horizontal(|ui| {
            ui.heading(TITLE);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(view.badge.text)
                        .strong()
                        .color(colour(view.badge.tone)),
                );
            });
        });

        if let Some(ro) = &view.read_only {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(ro.text).color(colour(Tone::Warn)));
                if ro.restart_button && ui.button("Restart as Administrator").clicked() {
                    out.push(Msg::RestartElevated);
                }
            });
        }

        egui::Frame::group(ui.style())
            .fill(colour(view.banner.tone).gamma_multiply(0.15))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(view.banner.text)
                        .strong()
                        .color(colour(view.banner.tone)),
                );
            });
        ui.add_space(4.0);

        egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
            for card in &view.cards {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&card.title).strong())
                            .on_hover_text(card.tooltip);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let mut on = card.toggle_on;
                            let toggle = ui
                                .add_enabled(view.controls_enabled, egui::Checkbox::new(&mut on, "Optimized"))
                                .on_hover_text(card.tooltip);
                            if toggle.changed() {
                                out.push(Msg::Request(Action::SetTweak {
                                    id: card.id,
                                    optimized: on,
                                }));
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label(&card.value);
                        ui.label(RichText::new(card.state_text).color(colour(card.tone)));
                    });
                    for d in &card.details {
                        ui.label(RichText::new(d).small());
                    }
                });
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(view.controls_enabled, egui::Button::new("Optimize Now"))
                .clicked()
            {
                out.push(Msg::Request(Action::ApplyAll));
            }
            if ui
                .add_enabled(view.controls_enabled, egui::Button::new("Revert to Defaults"))
                .clicked()
            {
                out.push(Msg::Request(Action::RevertAll));
            }
            if ui.button("Refresh").clicked() {
                out.push(Msg::Refresh);
            }
            if view.busy {
                ui.spinner();
            }
        });

        ui.separator();
        ui.label(RichText::new("Automation").strong());
        let mut auto_heal = view.auto_heal_checked;
        if ui
            .checkbox(&mut auto_heal, "Auto-heal settings if the OS resets them")
            .changed()
        {
            out.push(Msg::SetAutoHeal(auto_heal));
        }
        if let Some(hint) = view.auto_heal_hint {
            ui.label(RichText::new(hint).small().color(colour(Tone::Warn)));
        }
        let mut startup = view.startup_checked.unwrap_or(false);
        let startup_enabled = view.controls_enabled && view.startup_checked.is_some();
        if ui
            .add_enabled(
                startup_enabled,
                egui::Checkbox::new(&mut startup, "Run at startup with admin rights"),
            )
            .changed()
        {
            out.push(Msg::Request(Action::SetStartup(startup)));
        }
        let mut minimize = view.minimize_checked;
        if ui
            .checkbox(&mut minimize, "Minimize to tray instead of closing")
            .changed()
        {
            out.push(Msg::SetMinimizeToTray(minimize));
        }

        if let Some(text) = view.reboot_notice {
            ui.label(RichText::new(text).color(colour(Tone::Warn)));
        }
        if let Some(notice) = &view.notice {
            ui.horizontal(|ui| {
                let tone = match notice.kind {
                    NoticeKind::Info => Tone::Good,
                    NoticeKind::Error => Tone::Bad,
                };
                ui.label(RichText::new(&notice.text).color(colour(tone)));
                if ui.small_button("x").clicked() {
                    out.push(Msg::DismissNotice);
                }
            });
        }
        out
    }
}

impl eframe::App for Dashboard {
    /// Runs before every frame and also while hidden (woken by the worker
    /// and tray via `request_repaint`).
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut msgs: Vec<Msg> = self.ui_events.try_iter().collect();
        msgs.extend(self.worker.drain().into_iter().map(Msg::Worker));
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            msgs.push(Msg::CloseRequested);
        }
        for msg in msgs {
            self.dispatch(ctx, msg);
        }
        if self.pending_initial_hide {
            if self.painted {
                self.pending_initial_hide = false;
                if !self.state.window_visible {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
            } else {
                ctx.request_repaint();
            }
        }
        if let Some(tray) = &mut self.tray {
            tray.update(&TrayView::from_state(&self.state));
        }
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.painted = true;
        let ctx = ui.ctx().clone();
        let view = View::from_state(&self.state);
        let mut out = Vec::new();
        egui::CentralPanel::default().show(ui, |ui| {
            out = self.render(ui, &view);
        });
        for msg in out {
            self.dispatch(&ctx, msg);
        }
    }
}
