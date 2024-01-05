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
mod key_effect;
mod key_tracker;
mod midi_in;
mod midi_out;

/// Precision of the OS that can be trusted.
pub const SLEEP_PRECISION: Duration = Duration::from_millis(100);

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

    fn do_bloop_key(&self, mods: egui::Modifiers, i: usize, state: &UiState) {
        if let Some(bloop_state) = state.bloops.get(i) {
            if mods.shift {
                self.send(BloopCommand::ToggleListening(i));
            } else {
                if bloop_state.is_recording {
                    self.send(BloopCommand::StartPlaying(i));
                } else if bloop_state.is_playing_back {
                    self.send(BloopCommand::TogglePlayback(i));
                } else {
                    self.send(BloopCommand::StartRecording(i));
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Always refresh the UI.
        ctx.request_repaint();

        egui::CentralPanel::default().show(ctx, |ui| {
            let Ok(state) = self.ui_state_rx.recv() else {
                log::error!("Error fetching UI state");
                ui.colored_label(egui::Color32::RED, "Error fetching UI state");
                return;
            };

            ui.heading("Bloop.rs");
            ui.horizontal(|ui| {
                ui.allocate_space(egui::Vec2::new(0.0, 30.0));
                if let Some(duration) = state.duration {
                    if ui.small_button("Clear").clicked() {
                        self.send(BloopCommand::ClearAll);
                    }
                    ui.label(format!("Loop duration: {duration:?}"));
                }
            });
            for (i, bloop) in state.bloops.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Bloop #{i}"));

                    let r = ui.selectable_label(bloop.is_listening, "Listen");
                    if r.clicked() {
                        self.send(BloopCommand::ToggleListening(i));
                    }

                    let r = ui.selectable_label(bloop.is_playback_active, "Playback");
                    if r.clicked() {
                        self.send(BloopCommand::TogglePlayback(i));
                    }

                    if bloop.is_waiting_to_record {
                        ui.label("Waiting until start of loop ...");
                    } else if bloop.is_recording {
                        ui.label("Recording ...");
                        if state.duration.is_none() {
                            if ui.button("Stop recording").clicked() {
                                self.send(BloopCommand::StartPlaying(i));
                            }
                        }
                    } else if bloop.is_playing_back {
                        ui.label("Playing");
                        if ui.button("Cancel playback").clicked() {
                            self.send(BloopCommand::CancelPlaying(i));
                        }
                    } else {
                        ui.label("Idle");
                        if ui.button("Record").clicked() {
                            self.send(BloopCommand::StartRecording(i));
                        }
                    }
                });
            }

            ui.input(|input| {
                if input.key_pressed(egui::Key::Num1) {
                    self.do_bloop_key(input.modifiers, 0, &state);
                }
                if input.key_pressed(egui::Key::Num2) {
                    self.do_bloop_key(input.modifiers, 1, &state);
                }
                if input.key_pressed(egui::Key::Num3) {
                    self.do_bloop_key(input.modifiers, 2, &state);
                }
                if input.key_pressed(egui::Key::Num4) {
                    self.do_bloop_key(input.modifiers, 3, &state);
                }
                if input.key_pressed(egui::Key::Num5) {
                    self.do_bloop_key(input.modifiers, 4, &state);
                }
                if input.key_pressed(egui::Key::Num6) {
                    self.do_bloop_key(input.modifiers, 5, &state);
                }
                if input.key_pressed(egui::Key::Num7) {
                    self.do_bloop_key(input.modifiers, 6, &state);
                }
                if input.key_pressed(egui::Key::Num8) {
                    self.do_bloop_key(input.modifiers, 7, &state);
                }

                if input.key_pressed(egui::Key::Escape) {
                    self.send(BloopCommand::ClearAll);
                }
            });

            self.send(BloopCommand::RefreshUi);
        });
    }
}
