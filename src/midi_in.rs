use std::io::Write;

use eyre::{eyre, OptionExt, Result};
use midir::{Ignore, MidiIO, MidiInput, MidiInputConnection};

use crate::bloop::BloopCommand;

pub fn spawn_midi_in_thread(
    bloop_command_tx: flume::Sender<BloopCommand>,
) -> Result<MidiInputConnection<()>> {
    let mut midi_in = MidiInput::new("Bloop.rs Input")?;
    midi_in.ignore(Ignore::All);

    let in_port = select_port(&midi_in, "input")?;

    log::info!("Opening connections");

    let midi_input_connection = midi_in
        .connect(
            &in_port,
            "blooprs-forward",
            move |_timestamp, message, ()| match midly::live::LiveEvent::parse(message) {
                Ok(event) => _ = bloop_command_tx.send(event.to_static().into()),
                Err(e) => log::error!("Unable to parse MIDI message {message:x?}: {e}"),
            },
            (),
        )
        .map_err(|e| eyre!("{e}"))?;

    Ok(midi_input_connection)
}

fn select_port<T: MidiIO>(midi_io: &T, descr: &str) -> Result<T::Port> {
    let midi_ports = midi_io.ports();
    if let [port] = midi_ports.as_slice() {
        return Ok(port.clone());
    }

    println!("Available {} ports:", descr);
    for (i, p) in midi_ports.iter().enumerate() {
        println!("{}: {}", i, midi_io.port_name(p)?);
    }
    print!("Please select {} port: ", descr);
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let port = midi_ports
        .get(input.trim().parse::<usize>()?)
        .ok_or_eyre("Invalid port number")?;
    Ok(port.clone())
}
