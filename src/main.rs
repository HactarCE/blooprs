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

use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    time::{Duration, Instant},
};

use bloop::{BloopCommand, UiState};
use eframe::{egui, emath::NumExt};
use eyre::{eyre, OptionExt, Result};
use itertools::Itertools;
#[cfg(unix)]
use midir::os::unix::VirtualOutput;
use midir::{MidiIO, MidiInput, MidiInputConnection, MidiOutput};
use midly::live::LiveEvent;

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

const BLOOPRS_MIDI_OUTPUT_NAME: &str = "Bloop.rs Output";
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
    bloop_commands_tx: flume::Sender<BloopCommand>,

    midi_input: MidiInput,
    midi_input_connections: Vec<MidiInputConnectionHandle>,

    midi_output: MidiOutput,
    midi_output_port_name: Option<String>,

    ui_state_rx: flume::Receiver<UiState>,
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>) -> Result<Self> {
        let (bloop_commands_tx, bloop_commands_rx) = flume::unbounded();

        let mut midi_in = MidiInput::new("Bloop.rs Input")?;
        midi_in.ignore(midir::Ignore::All);

        let ui_state_rx =
            crate::bloop::spawn_bloops_thread(bloop_commands_tx.clone(), bloop_commands_rx)?;

        let mut app = App {
            bloop_commands_tx,

            midi_input: new_midi_input(),
            midi_input_connections: vec![],

            midi_output: new_midi_output(),
            midi_output_port_name: None,

            ui_state_rx,
        };

        app.refresh_midi_input_connections();
        app.refresh_midi_output_connections();
        #[cfg(unix)]
        if let Err(e) = app.open_midi_output_connection(BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME) {
            log::error!("error opening MIDI output connection: {e}")
        }

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
                self.send(BloopCommand::DoKey(i));
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

            ui.group(|ui| self.midi_io_ui(ui));

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

impl App {
    fn midi_io_ui(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());

        ui.horizontal(|ui| {
            ui.label("MIDI inputs:");

            for conn in &self.midi_input_connections {
                if ui.selectable_label(conn.is_enabled(), &conn.name).clicked() {
                    conn.toggle();
                }
            }

            if ui.button("⟳").on_hover_text("Refresh").clicked() {
                self.refresh_midi_input_connections();
            }
        });

        ui.horizontal(|ui| {
            ui.label("MIDI outputs:");

            let mut port_names = port_names(&self.midi_output);
            #[cfg(unix)]
            port_names.insert(0, BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME.to_owned());
            for port_name in port_names {
                let is_selected = Some(&port_name) == self.midi_output_port_name.as_ref();
                if ui.selectable_label(is_selected, &port_name).clicked() {
                    match self.open_midi_output_connection(&port_name) {
                        Ok(()) => (),
                        Err(e) => log::error!("error opening MIDI output connection: {e}"),
                    }
                }
            }

            if ui.button("⟳").on_hover_text("Refresh").clicked() {
                self.refresh_midi_output_connections();
            }
        });
    }

    fn refresh_midi_input_connections(&mut self) {
        let previously_disabled_ports: HashSet<String> =
            std::mem::take(&mut self.midi_input_connections)
                .into_iter()
                .filter(|port| !port.is_enabled())
                .map(|port| port.name)
                .collect();

        self.midi_input = new_midi_input();

        for port_name in port_names(&self.midi_input) {
            if port_name == BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME {
                continue;
            }
            let is_enabled = !previously_disabled_ports.contains(&port_name);
            match self.open_midi_input_connection(&port_name, is_enabled) {
                Ok(midi_input_connection) => {
                    self.midi_input_connections.push(midi_input_connection)
                }
                Err(e) => log::error!("error opening MIDI input connection: {e}"),
            }
        }
    }
    fn refresh_midi_output_connections(&mut self) {
        self.midi_output = new_midi_output();

        if let Some(port_name) = self.midi_output_port_name.clone() {
            match self.open_midi_output_connection(&port_name) {
                Ok(()) => (),
                Err(e) => log::error!("error opening MIDI output connection: {e}"),
            }
        }
    }
    fn open_midi_input_connection(
        &self,
        port_name: &str,
        is_enabled: bool,
    ) -> Result<MidiInputConnectionHandle> {
        let midi_input = MidiInput::new(&format!("Bloop.rs {port_name:?} Input"))?;
        let port = find_port(&midi_input, port_name)?;

        let tx = self.bloop_commands_tx.clone();

        let is_enabled = Arc::new(AtomicBool::new(is_enabled));
        let is_enabled_ref = Arc::clone(&is_enabled);

        let connection = midi_input
            .connect(
                &port,
                "blooprs-in",
                move |_timestamp, message: &[u8], ()| {
                    if is_enabled_ref.load(std::sync::atomic::Ordering::Relaxed) {
                        match midly::live::LiveEvent::parse(message) {
                            Ok(event) => _ = tx.send(event.to_static().into()),
                            Err(e) => log::error!("unable to parse MIDI message {message:x?}: {e}"),
                        }
                    }
                },
                (),
            )
            .map_err(|e| eyre!("{e}"))?;

        Ok(MidiInputConnectionHandle {
            name: port_name.to_owned(),
            is_enabled,
            connection,
        })
    }
    fn open_midi_output_connection(&mut self, port_name: &str) -> Result<()> {
        let midi_output = new_midi_output();

        #[cfg(unix)]
        let mut midi_output_connection = if port_name == BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME {
            midi_output.create_virtual(BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME)
        } else {
            let port = find_port(&midi_output, port_name)?;
            midi_output.connect(&port, "blooprs-out")
        }
        .map_err(|e| eyre!("{e}"))?;
        #[cfg(not(unix))]
        let mut midi_output_connection = {
            let port = find_port(&midi_output, port_name)?;
            midi_output.connect(&port, "blooprs-out")
        }
        .map_err(|e| eyre!("{e}"))?;

        let (midi_out_tx, midi_out_rx) = flume::unbounded::<LiveEvent<'static>>();

        std::thread::spawn(move || {
            let mut buffer = vec![];
            for event in midi_out_rx {
                buffer.clear();
                if let Err(e) = event.write(&mut buffer) {
                    log::error!("Error writing MIDI event to buffer: {e}");
                    continue;
                }
                if let Err(e) = midi_output_connection.send(&buffer) {
                    log::error!("Error sending MIDI event to output: {e}");
                    continue;
                }
            }
            drop(midi_output_connection);
        });

        self.send(BloopCommand::SetMidiOutput(midi_out_tx));
        self.midi_output_port_name = Some(port_name.to_owned());
        Ok(())
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

struct MidiInputConnectionHandle {
    name: String,
    is_enabled: Arc<AtomicBool>,
    connection: MidiInputConnection<()>,
}
impl MidiInputConnectionHandle {
    pub fn toggle(&self) {
        self.is_enabled.fetch_xor(true, Ordering::Relaxed);
    }
    pub fn is_enabled(&self) -> bool {
        self.is_enabled.load(Ordering::Relaxed)
    }
}

fn new_midi_input() -> MidiInput {
    let mut midi_input = MidiInput::new("Bloop.rs").expect("error creating MIDI input");
    midi_input.ignore(midir::Ignore::All);
    midi_input
}

fn new_midi_output() -> MidiOutput {
    MidiOutput::new("bloo").expect("error creating MIDI output")
}

fn port_names<T: MidiIO>(midi_io: &T) -> Vec<String> {
    let mut names = midi_io
        .ports()
        .iter()
        .filter_map(|port| midi_io.port_name(port).ok())
        .collect_vec();
    names.sort();
    names
}
fn find_port<T: MidiIO>(midi_io: &T, port_name: &str) -> Result<T::Port> {
    midi_io
        .ports()
        .into_iter()
        .find(|port| midi_io.port_name(port).is_ok_and(|s| s == port_name))
        .ok_or_eyre("unabled to find port")
}
