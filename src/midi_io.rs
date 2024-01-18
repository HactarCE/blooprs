use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui;
use eyre::{eyre, OptionExt, Result};
use itertools::Itertools;
#[cfg(unix)]
use midir::os::unix::VirtualOutput;
use midir::{MidiIO, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use midly::live::LiveEvent;
use parking_lot::Mutex;

use crate::{APP_NAME, BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME};

/// MIDI input/output handlers for the app.
pub struct AppMidiIO<T> {
    input: MidiInput,
    input_connections: Vec<MidiInputConnectionHandle>,
    input_tx: flume::Sender<T>,

    output: MidiOutput,
    output_port_name: Option<String>,
    output_connection: Arc<Mutex<Option<MidiOutputConnection>>>,
}
impl<T: 'static + Send> AppMidiIO<T>
where
    for<'a> LiveEvent<'a>: Into<T>,
{
    pub fn new(
        midi_in_tx: flume::Sender<T>,
        midi_out_rx: flume::Receiver<LiveEvent<'static>>,
    ) -> Self {
        let output_connection = Arc::new(Mutex::new(None));
        let output_connection_ref = Arc::clone(&output_connection);

        let mut ret = Self {
            input: new_midi_input(),
            input_connections: vec![],
            input_tx: midi_in_tx,

            output: new_midi_output(),
            output_port_name: None,
            output_connection,
        };

        ret.refresh_midi_input_connections();
        ret.refresh_midi_output_connections();

        // Spawn output thread.
        std::thread::spawn(move || {
            let mut buffer = vec![];
            for event in midi_out_rx {
                buffer.clear();
                if let Err(e) = event.write(&mut buffer) {
                    log::error!("Error writing MIDI event to buffer: {e}");
                    continue;
                }
                let mut out_conn_guard = output_connection_ref.lock();
                if let Some(out_conn) = &mut *out_conn_guard {
                    if let Err(e) = out_conn.send(&buffer) {
                        log::error!("Error sending MIDI event to output: {e}");
                        continue;
                    }
                }
            }
            drop(output_connection_ref);
        });

        ret
    }

    pub fn refresh_midi_input_connections(&mut self) {
        let previously_disabled_ports: HashSet<String> =
            std::mem::take(&mut self.input_connections)
                .into_iter()
                .filter(|port| !port.is_enabled())
                .map(|port| port.name)
                .collect();

        self.input = new_midi_input();

        for port_name in port_names(&self.input) {
            if port_name == crate::BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME {
                continue;
            }
            let is_enabled = !previously_disabled_ports.contains(&port_name);
            match self.open_midi_input_connection(&port_name, is_enabled) {
                Ok(midi_input_connection) => self.input_connections.push(midi_input_connection),
                Err(e) => log::error!("error opening MIDI input connection: {e}"),
            }
        }
    }
    pub fn refresh_midi_output_connections(&mut self) {
        self.output = new_midi_output();

        #[cfg(unix)]
        if self.output_port_name.is_none() {
            self.output_port_name = Some(BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME.to_owned());
        }

        if let Some(output_port_name) = self.output_port_name.take() {
            self.open_output_connection(&output_port_name);
        }
    }
    fn open_midi_input_connection(
        &self,
        port_name: &str,
        is_enabled: bool,
    ) -> Result<MidiInputConnectionHandle> {
        let midi_input = MidiInput::new(&format!("Bloop.rs {port_name:?} Input"))?;
        let port = find_port(&midi_input, port_name)?;

        let is_enabled = Arc::new(AtomicBool::new(is_enabled));
        let is_enabled_ref = Arc::clone(&is_enabled);

        let midi_input_tx = self.input_tx.clone();

        let _connection = midi_input
            .connect(
                &port,
                "blooprs-in",
                move |_timestamp, message: &[u8], ()| {
                    if is_enabled_ref.load(std::sync::atomic::Ordering::Relaxed) {
                        match midly::live::LiveEvent::parse(message) {
                            Ok(event) => _ = midi_input_tx.send(event.into()),
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
            _connection,
        })
    }
    pub fn open_output_connection(&mut self, port_name: &str) {
        match self.open_output_connection_internal(port_name) {
            Ok(out_conn) => {
                self.output_port_name = Some(port_name.to_owned());
                *self.output_connection.lock() = Some(out_conn);
            }
            Err(e) => {
                self.output_port_name = None;
                *self.output_connection.lock() = None;
                log::error!("error opening MIDI output connection: {e}");
            }
        }
    }
    fn open_output_connection_internal(&mut self, port_name: &str) -> Result<MidiOutputConnection> {
        let midi_output = new_midi_output();

        #[cfg(unix)]
        let out_conn = if port_name == BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME {
            midi_output.create_virtual(BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME)
        } else {
            let port = find_port(&midi_output, port_name)?;
            midi_output.connect(&port, "blooprs-out")
        }
        .map_err(|e| eyre!("{e}"))?;
        #[cfg(not(unix))]
        let out_conn = {
            let port = find_port(&midi_output, port_name)?;
            midi_output.connect(&port, "blooprs-out")
        }
        .map_err(|e| eyre!("{e}"))?;

        Ok(out_conn)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<flume::Sender<T>> {
        let new_output_tx = None;

        ui.set_width(ui.available_width());

        ui.horizontal(|ui| {
            ui.label("MIDI inputs:");

            for conn in &self.input_connections {
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

            let mut port_names = port_names(&self.output);
            #[cfg(unix)]
            port_names.insert(0, BLOOPRS_MIDI_VIRTUAL_OUTPUT_NAME.to_owned());
            for port_name in port_names {
                let is_selected = Some(&port_name) == self.output_port_name.as_ref();
                if ui.selectable_label(is_selected, &port_name).clicked() {
                    return self.open_output_connection(&port_name);
                }
            }

            if ui.button("⟳").on_hover_text("Refresh").clicked() {
                self.refresh_midi_output_connections();
            }
        });

        new_output_tx
    }
}

/// Handle to an active MIDI connection.
pub struct MidiInputConnectionHandle {
    /// Name of the connection that is displayed to the user.
    pub name: String,
    /// Whether the application is listening to this MIDI input.
    is_enabled: Arc<AtomicBool>,
    /// The MIDI input callback will be called until this field is dropped.
    _connection: MidiInputConnection<()>,
}
impl MidiInputConnectionHandle {
    /// Toggles whether the application is listening to this MIDI input.
    pub fn toggle(&self) {
        self.is_enabled.fetch_xor(true, Ordering::Relaxed);
    }
    /// Returns whether the application is listening to this MIDI input.
    pub fn is_enabled(&self) -> bool {
        self.is_enabled.load(Ordering::Relaxed)
    }
}

/// Returns a new `MidiInput`.
pub fn new_midi_input() -> MidiInput {
    let mut midi_input =
        MidiInput::new(&format!("{APP_NAME} Input")).expect("error creating MIDI input");
    midi_input.ignore(midir::Ignore::All);
    midi_input
}

/// Returns a new `MidiOutput`.
pub fn new_midi_output() -> MidiOutput {
    MidiOutput::new(&format!("{APP_NAME} Output")).expect("error creating MIDI output")
}

/// Returns a list of the names of the MIDI ports on `midi_io`.
fn port_names<T: MidiIO>(midi_io: &T) -> Vec<String> {
    let mut names = midi_io
        .ports()
        .iter()
        .filter_map(|port| midi_io.port_name(port).ok())
        .collect_vec();
    names.sort();
    names
}
/// Returns a handle for the first port on `midi_io` that has the given name.
fn find_port<T: MidiIO>(midi_io: &T, port_name: &str) -> Result<T::Port> {
    midi_io
        .ports()
        .into_iter()
        .find(|port| midi_io.port_name(port).is_ok_and(|s| s == port_name))
        .ok_or_eyre("unabled to find port")
}
