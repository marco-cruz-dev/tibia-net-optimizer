//! The dashboard window (eframe/egui).

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};

use super::tray::Tray;
use crate::actions::Action;
use crate::app::icon::{app_icon_rgba, tone_rgb, WINDOW_ICON_SIZE};
use crate::app::ping::{GameProbe, PingMonitor, PING_INTERVAL, PROBE_TIMEOUT};
use crate::app::state::{AppState, Effect, Msg, NoticeKind, PING_WINDOW};
use crate::app::view::{diagnostics, PingView, Tone, TrayView, View, PING_EXPLAINER};
use crate::app::worker::Worker;
use crate::config::Config;
use crate::network::gameconn::{detect_game_targets, system_connections};
use crate::network::probe::icmp_echo;
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
        rgba: app_icon_rgba(WINDOW_ICON_SIZE),
        width: WINDOW_ICON_SIZE,
        height: WINDOW_ICON_SIZE,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size([420.0, 600.0])
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
    /// Lets background threads (tray, ping monitor) post messages.
    ui_tx: Sender<Msg>,
    ping: Option<PingMonitor>,
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
        // All text can be selected and copied (Ctrl+C) with the clipboard feature.
        ctx.all_styles_mut(|s| s.interaction.selectable_labels = true);
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

        let tray = match Tray::new(tx.clone(), ctx.clone()) {
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
            ui_tx: tx,
            ping: None,
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
            Effect::RestartElevated => {
                // Relaunch this (dashboard) exe elevated, with no arguments.
                let result = std::env::current_exe()
                    .map_err(crate::Error::from)
                    .and_then(|exe| elevation::relaunch_elevated(&exe, &[]));
                match result {
                    Ok(()) => self.quit(ctx),
                    Err(e) => self.dispatch(ctx, Msg::RestartFailed(e.to_string())),
                }
            }
            Effect::Notify { title, body } => {
                // Off the UI thread: toasts and notify-send can be slow.
                std::thread::spawn(move || {
                    if let Err(e) = notify::send(&title, &body) {
                        eprintln!("Notification failed: {e}");
                    }
                });
            }
            Effect::StartPing { generation } => {
                let (tx, wake) = (self.ui_tx.clone(), ctx.clone());
                let source = system_connections();
                let mut probe = GameProbe::new(
                    move || detect_game_targets(source.as_ref()),
                    |ip| icmp_echo(ip, PROBE_TIMEOUT),
                );
                // Replacing the monitor drops (stops) the previous one.
                self.ping = Some(PingMonitor::start(
                    generation,
                    Box::new(move || probe.tick()),
                    PING_INTERVAL,
                    move |sample| {
                        let _ = tx.send(Msg::Ping(sample));
                        wake.request_repaint();
                    },
                ));
            }
            Effect::StopPing => self.ping = None,
            Effect::Quit => self.quit(ctx),
        }
    }

    fn render_ping(ui: &mut egui::Ui, p: &PingView) {
        ui.label(RichText::new("Live Connection Monitor").strong())
            .on_hover_text(PING_EXPLAINER);
        ui.label(RichText::new(p.status).small())
            .on_hover_text(PING_EXPLAINER);
        if let Some(target) = &p.target {
            ui.label(
                RichText::new(format!("Entry point: {target} (ICMP)"))
                    .small()
                    .weak(),
            );
        }
        egui::Grid::new("ping-stats")
            .num_columns(4)
            .spacing([18.0, 2.0])
            .show(ui, |ui| {
                for label in ["Ping", "Average", "Jitter", "Loss"] {
                    ui.label(RichText::new(label).small().weak());
                }
                ui.end_row();
                ui.label(RichText::new(&p.current).strong());
                ui.label(RichText::new(&p.average).strong());
                ui.label(RichText::new(&p.jitter).strong().color(colour(p.jitter_tone)));
                ui.label(RichText::new(&p.loss).strong().color(colour(p.loss_tone)));
                ui.end_row();
            });
        sparkline(ui, &p.points);
        if let Some(e) = &p.error {
            ui.label(
                RichText::new(format!("Last probe failed: {e}"))
                    .small()
                    .color(colour(Tone::Bad)),
            );
        }
    }

    fn quit(&mut self, ctx: &egui::Context) {
        self.allow_close = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn render(&self, ui: &mut egui::Ui, view: &View, ping: &PingView) -> Vec<Msg> {
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

        egui::ScrollArea::vertical().max_height(230.0).show(ui, |ui| {
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
        Self::render_ping(ui, ping);

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
        ui.separator();
        if ui
            .button("Copy diagnostics")
            .on_hover_text("Copies a text summary of everything shown here, for sharing or bug reports.")
            .clicked()
        {
            ui.ctx().copy_text(diagnostics(&self.state));
            out.push(Msg::DiagnosticsCopied);
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

/// RTT history: one slot per sample in the stats window, newest on the right.
/// Lost probes are red ticks along the bottom.
fn sparkline(ui: &mut egui::Ui, points: &[Option<f64>]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 48.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let values: Vec<f64> = points.iter().flatten().copied().collect();
    if values.is_empty() {
        return;
    }
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min) * 0.9;
    let hi = (values.iter().copied().fold(f64::NEG_INFINITY, f64::max) * 1.1).max(lo + 1.0);
    let step = rect.width() / (PING_WINDOW - 1) as f32;
    let offset = PING_WINDOW.saturating_sub(points.len());
    let x = |i: usize| rect.left() + (offset + i) as f32 * step;
    let y = |v: f64| rect.bottom() - 4.0 - ((v - lo) / (hi - lo)) as f32 * (rect.height() - 8.0);

    let line = egui::Stroke::new(1.5, colour(Tone::Good));
    let mut segment: Vec<egui::Pos2> = Vec::new();
    for (i, p) in points.iter().enumerate() {
        match p {
            Some(v) => segment.push(egui::pos2(x(i), y(*v))),
            None => {
                if segment.len() > 1 {
                    painter.add(egui::Shape::line(std::mem::take(&mut segment), line));
                }
                segment.clear();
                let bx = x(i);
                painter.line_segment(
                    [egui::pos2(bx, rect.bottom() - 6.0), egui::pos2(bx, rect.bottom())],
                    egui::Stroke::new(2.0, colour(Tone::Bad)),
                );
            }
        }
    }
    if segment.len() > 1 {
        painter.add(egui::Shape::line(segment, line));
    } else if let Some(p) = segment.first() {
        painter.circle_filled(*p, 2.0, colour(Tone::Good));
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
            out = self.render(ui, &view, &PingView::from_state(&self.state));
        });
        for msg in out {
            self.dispatch(&ctx, msg);
        }
    }
}
