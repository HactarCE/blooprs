use eyre::{eyre, Result};
use midir::os::unix::VirtualOutput;
use midir::MidiOutput;
use midly::live::LiveEvent;

pub fn spawn_midi_out_thread() -> Result<flume::Sender<LiveEvent<'static>>> {
    let (midi_out_tx, midi_out_rx) = flume::unbounded::<LiveEvent<'static>>();
    let midi_out = MidiOutput::new("Bloop.rs Output")?;
    let mut conn_out = midi_out
        .create_virtual("Bloop.rs")
        .map_err(|e| eyre!("{e}"))?;

    std::thread::spawn(move || {
        let mut buffer = vec![];
        for event in midi_out_rx {
            buffer.clear();
            if let Err(e) = event.write(&mut buffer) {
                log::error!("Error writing MIDI event to buffer: {e}");
                continue;
            }
            if let Err(e) = conn_out.send(&buffer) {
                log::error!("Error sending MIDI event to output: {e}");
                continue;
            }
        }
        drop(conn_out);
    });

    Ok(midi_out_tx)
}
