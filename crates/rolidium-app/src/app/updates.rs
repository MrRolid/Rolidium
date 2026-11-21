// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The notice of a new version (see docs/PRIVACY.md). Once a day, in the background, with a time limit, and only when the person has said yes:
//! a question to rolidium.com for the newest version. The program never downloads or starts anything: the button opens the fixed download page
//! in the browser.

use super::*;
use std::sync::mpsc::{channel, Receiver};

#[derive(Default)]
pub(super) struct UpdateState {
    rx: Option<Receiver<Result<rolidium_net::Latest, String>>>,
    /// The newer version that is on offer (not skipped).
    pub available: Option<String>,
    /// Do not try again in this run before this time (a failed question is not repeated every frame).
    tried: bool,
}

/// Days since 1970 (UTC).
pub(super) fn today() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400)
}

/// Whether the question may be asked now: the person said yes, there is no `ROLIDIUM_OFFLINE`, it is a normal window (not a self test or a
/// benchmark), and the last question was on an earlier day.
pub(super) fn question_due(
    consent: Option<bool>,
    offline: bool,
    normal_mode: bool,
    last_day: u64,
    today: u64,
) -> bool {
    consent == Some(true) && !offline && normal_mode && last_day < today
}

/// Whether the consent card is shown: the person has not answered, and no network is possible anyway when it is switched off.
pub(super) fn ask_for_consent(consent: Option<bool>, offline: bool, normal_mode: bool) -> bool {
    consent.is_none() && !offline && normal_mode
}

impl App {
    fn normal_mode(&self) -> bool {
        self.selftest.is_none()
            && self.bench.is_none()
            && !BENCH_RUN.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Once a frame: starts the question when it is due and takes in the answer.
    pub(super) fn updates_tick(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.update.rx {
            if let Ok(r) = rx.try_recv() {
                self.update.rx = None;
                match r {
                    Ok(latest) => {
                        if rolidium_net::is_newer(&latest.latest, env!("CARGO_PKG_VERSION"))
                            && latest.latest != self.settings.skip_version
                        {
                            self.update.available = Some(latest.latest);
                        }
                    }
                    // a network error is not shown to the person
                    Err(e) => log(&format!("version check: {e}")),
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
        if self.update.rx.is_some() || self.update.tried || !crate::startup::frames_drawn() {
            return;
        }
        let day = today();
        if !question_due(
            self.settings.check_updates,
            rolidium_net::offline(),
            self.normal_mode(),
            self.settings.last_check_day,
            day,
        ) {
            return;
        }
        // the day of the question is written before it is asked: at most one a day, whatever comes of it
        self.update.tried = true;
        self.settings.last_check_day = day;
        self.settings.save();
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(rolidium_net::fetch_latest(env!("CARGO_PKG_VERSION")));
            c.request_repaint();
        });
        self.update.rx = Some(rx);
    }

    /// The small cards: the question about the consent (first start) and the notice of a new version.
    #[allow(deprecated)]
    pub(super) fn updates_ui(&mut self, ctx: &egui::Context) {
        let vp = self.viewport_rect;
        if vp.width() < 50.0 {
            return;
        }
        if ask_for_consent(
            self.settings.check_updates,
            rolidium_net::offline(),
            self.normal_mode(),
        ) {
            let mut answer = None;
            egui::Area::new(egui::Id::new("update_consent"))
                .order(egui::Order::Foreground)
                .fixed_pos(Pos2::new((vp.right() - 352.0).max(vp.left() + 8.0), vp.top() + 12.0))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(330.0);
                        ui.label(egui::RichText::new(tr("Check for a new version?")).strong());
                        ui.label(tr("Once a day Rolidium can ask rolidium.com in the background whether a newer version exists. It sends only the version of the program, the name and main number of your system and the type of processor: no identifier, no cookies, no data about your hardware, no file names. Until you answer, Rolidium does not connect to the network."));
                        ui.horizontal(|ui| {
                            if ui.button(tr("Yes")).clicked() {
                                answer = Some(true);
                            }
                            if ui.button(tr("No")).clicked() {
                                answer = Some(false);
                            }
                            if ui.link(tr("Privacy notice")).clicked() {
                                crate::diagnose::open_path(std::path::Path::new(rolidium_net::urls::PRIVACY));
                            }
                        });
                    });
                });
            if let Some(a) = answer {
                self.settings.check_updates = Some(a);
                self.settings.save();
            }
        }
        if let Some(v) = self.update.available.clone() {
            let mut close = false;
            egui::Area::new(egui::Id::new("update_notice"))
                .order(egui::Order::Foreground)
                .fixed_pos(Pos2::new(
                    (vp.right() - 332.0).max(vp.left() + 8.0),
                    (vp.bottom() - 70.0).max(vp.top() + 8.0),
                ))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(310.0);
                        ui.label(trf("Version {} is available", std::slice::from_ref(&v)));
                        ui.horizontal(|ui| {
                            // the address is the fixed one of the program, never one from the answer of the server
                            if ui.button(tr("Download")).clicked() {
                                crate::diagnose::open_path(std::path::Path::new(
                                    rolidium_net::urls::DOWNLOAD,
                                ));
                            }
                            if ui.button(tr("Skip this version")).clicked() {
                                self.settings.skip_version = v.clone();
                                self.settings.save();
                                close = true;
                            }
                            if ui.small_button("×").on_hover_text(tr("Close")).clicked() {
                                close = true;
                            }
                        });
                    });
                });
            if close {
                self.update.available = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_question_is_asked_only_with_consent_once_a_day_and_never_offline() {
        // no answer yet: no question, whatever else
        assert!(!question_due(None, false, true, 0, 20_000));
        assert!(!question_due(Some(false), false, true, 0, 20_000));
        assert!(question_due(Some(true), false, true, 19_999, 20_000));
        // the same day: not again
        assert!(!question_due(Some(true), false, true, 20_000, 20_000));
        // ROLIDIUM_OFFLINE, a self test or a benchmark: never
        assert!(!question_due(Some(true), true, true, 0, 20_000));
        assert!(!question_due(Some(true), false, false, 0, 20_000));
    }

    #[test]
    fn the_consent_card_is_for_the_first_start_of_a_normal_window_only() {
        assert!(ask_for_consent(None, false, true));
        assert!(!ask_for_consent(Some(true), false, true));
        assert!(!ask_for_consent(Some(false), false, true));
        assert!(
            !ask_for_consent(None, true, true),
            "with ROLIDIUM_OFFLINE the question is not even asked"
        );
        assert!(
            !ask_for_consent(None, false, false),
            "not in a self test or a benchmark"
        );
    }
}
