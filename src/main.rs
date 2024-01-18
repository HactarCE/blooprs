//! Opinionated MIDI looper.

use std::time::{Duration, Instant};

use bloop::{BloopCommand, UiState};
use eframe::egui;
use eframe::emath::NumExt;
use eyre::{eyre, Context, Result};
use midi_io::AppMidiIO;

#[macro_use]
mod generic_vec;
mod bloop;
mod key_effect;
mod key_tracker;
mod midi_io;

/// Precision of the OS that can be trusted.
pub const SLEEP_PRECISION: Duration = Duration::from_millis(100);

pub const APP_NAME: &str = "Bloop.rs";

/// Whether to send note-on events whenever a key is pressed, even if the
/// corresponding note-off event might not be sent.
pub const ALLOW_UNMATCHED_NOTE_ON: bool = true;

/// Name for the application's virtual MIDI output.
#[cfg(unix)]
const BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME: &str = "Bloop.rs Virtual Output";

fn main() -> Result<()> {
    // Initialize logging.
    env_logger::builder().init();

    // Initialize panic handler.
    // #[cfg(debug_assertions)]
    // color_eyre::install()?;

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
    midi_io: AppMidiIO<BloopCommand>,
    bloop_commands_tx: flume::Sender<BloopCommand>,

    ui_state_rx: flume::Receiver<UiState>,
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>) -> Result<Self> {
        let (bloop_commands_tx, ui_state_rx, midi_out_rx) = crate::bloop::spawn_bloops_thread()?;

        let midi_io = AppMidiIO::new(bloop_commands_tx.clone(), midi_out_rx);

        Ok(App {
            bloop_commands_tx,

            midi_io,

            ui_state_rx,
        })
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
                self.send(BloopCommand::DoKey(i));
            }
        }
    }

    fn latest_ui_state(&self) -> Result<UiState> {
        if self.ui_state_rx.is_empty() {
            self.send(BloopCommand::RefreshUi);
        }
        self.ui_state_rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .wrap_err("error fetching UI state")
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Always refresh the UI.
        ctx.request_repaint();

        egui::CentralPanel::default().show(ctx, |ui| {
            let state = match self.latest_ui_state() {
                Ok(s) => s,
                Err(e) => {
                    log::error!("error fetching UI state: {e}");
                    ui.colored_label(egui::Color32::RED, "Error fetching UI state");
                    return;
                }
            };

            ui.heading("Bloop.rs");

            ui.group(|ui| self.midi_io.ui(ui));

            draw_time_display(ui, &state);

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

fn draw_time_display(ui: &mut egui::Ui, state: &UiState) {
    const MARGIN: f32 = 5.0;

    let measures_per_loop = 8;
    let beats_per_measure = 4;

    let beat_count = measures_per_loop * beats_per_measure;
    let beat_width = (ui.available_width().at_most(500.0) / beat_count as f32).floor();
    let measure_width = beat_width * beats_per_measure as f32;
    let total_width = measure_width * measures_per_loop as f32;

    let loop_display_size = egui::vec2(total_width + MARGIN * 2.0, 130.0);

    let (r, painter) = ui.allocate_painter(loop_display_size, egui::Sense::click());
    let rect = r.rect.expand(-MARGIN);
    let vline = |p: &egui::Painter, x: f32, h: f32, color: egui::Color32| {
        p.line_segment(
            [
                rect.lerp_inside(egui::vec2(x, 0.5 - 0.5 * h)),
                rect.lerp_inside(egui::vec2(x, 0.5 + 0.5 * h)),
            ],
            egui::Stroke { width: 2.0, color },
        )
    };

    let measure_width = 1.0 / measures_per_loop as f32;
    let beat_width = measure_width / beats_per_measure as f32;

    for i in 0..measures_per_loop {
        let measure_x = i as f32 * measure_width;
        vline(&painter, measure_x, 1.0, egui::Color32::GRAY);
        for j in 1..beats_per_measure {
            vline(
                &painter,
                j as f32 * beat_width + measure_x,
                0.75,
                egui::Color32::DARK_GRAY,
            )
        }
    }
    vline(&painter, 1.0, 1.0, egui::Color32::GRAY);

    if let (Some(epoch), Some(duration)) = (state.epoch, state.duration) {
        let x = ((Instant::now() - epoch).as_secs_f32() / duration.as_secs_f32()).fract();
        vline(&painter, x, 1.0, egui::Color32::LIGHT_BLUE);
    }
}
