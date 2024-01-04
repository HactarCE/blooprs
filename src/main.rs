//! Opinionated MIDI looper.

#![warn(
    rust_2018_idioms,
    missing_docs,
    clippy::if_then_some_else_none,
    clippy::manual_let_else,
    clippy::semicolon_if_nothing_returned,
    clippy::semicolon_inside_block,
    clippy::too_many_lines,
    clippy::undocumented_unsafe_blocks,
    clippy::unwrap_used
)]
#![deny(clippy::correctness)]

use std::time::Duration;

use bloop::{BloopCommand, UiState};
use eframe::egui;
use eyre::{eyre, Result};
use midir::MidiInputConnection;

#[macro_use]
mod generic_vec;
mod bloop;
mod midi_in;
mod midi_out;

/// Precision of the OS that can be trusted.
pub const SLEEP_PRECISION: Duration = Duration::from_millis(100);

/// Time interval before starting a recording during which MIDI events are
/// included in the recorded.
pub const LOOKBACK_TIME: Duration = Duration::from_millis(100);

/// Whether to send note-on events whenever a key is pressed, even if the
/// corresponding note-off event might not be sent.
pub const ALLOW_UNMATCHED_NOTE_ON: bool = true;

fn main() -> Result<()> {
    // Initialize logging.
    env_logger::builder().init();

    // Initialize panic handler.
    #[cfg(debug_assertions)]
    color_eyre::install()?;

    // Run the GUI.
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Bloop.rs",
        native_options,
        Box::new(|cc| Box::new(App::new(cc).unwrap())),
    )
    .map_err(|e| eyre!("{e}"))
}

struct App {
    bloop_commands_tx: flume::Sender<BloopCommand>,
    _midi_input_connection: MidiInputConnection<()>,
    ui_state_rx: flume::Receiver<UiState>,
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>) -> Result<Self> {
        let (bloop_commands_tx, bloop_commands_rx) = flume::unbounded();
        let _midi_input_connection =
            crate::midi_in::spawn_midi_in_thread(bloop_commands_tx.clone())?;
        let ui_state_rx = crate::bloop::spawn_bloops_thread(bloop_commands_rx)?;

        let app = App {
            bloop_commands_tx,
            _midi_input_connection,
            ui_state_rx,
        };
        app.send(BloopCommand::RefreshUi);
        Ok(app)
    }

    fn send(&self, command: BloopCommand) {
        if let Err(e) = self.bloop_commands_tx.send(command) {
            log::error!("Error sending command: {e}");
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let Ok(state) = self.ui_state_rx.recv() else {
                log::error!("Error fetching UI state");
                ui.colored_label(egui::Color32::RED, "Error fetching UI state");
                return;
            };

            ui.heading("Bloop.rs");
            match state.duration {
                Some(duration) => ui.label(format!("Loop duration: {duration:?}")),
                None => ui.label(""),
            };
            for (i, bloop) in state.bloops.iter().enumerate() {
                ui.horizontal(|ui| {
                    let r = ui.selectable_label(bloop.is_active, format!("Bloop #{i}"));
                    if r.clicked() {
                        self.send(BloopCommand::ToggleActive(i));
                    }

                    match bloop.state {
                        bloop::BloopState::Idle => {
                            ui.label("Idle");
                            if ui.button("Start recording").clicked() {
                                self.send(BloopCommand::StartRecording(i));
                            }
                        }
                        bloop::BloopState::Recording { start: _, end } => {
                            ui.label("Recording ...");
                            if end.is_none() {
                                if ui.button("Stop recording").clicked() {
                                    self.send(BloopCommand::StartPlaying(i));
                                }
                            }
                        }
                        bloop::BloopState::Playing { .. } => {
                            ui.label("Playing");
                            if ui.button("Clear").clicked() {
                                self.send(BloopCommand::Clear(i));
                            }
                        }
                    }
                });
            }

            self.send(BloopCommand::RefreshUi);
        });
    }
}
