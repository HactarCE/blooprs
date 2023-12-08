#![warn(
    rust_2018_idioms,
    // missing_docs,
    clippy::if_then_some_else_none,
    clippy::manual_let_else,
    clippy::semicolon_if_nothing_returned,
    clippy::semicolon_inside_block,
    clippy::too_many_lines,
    clippy::undocumented_unsafe_blocks,
    clippy::unwrap_used
)]
#![deny(clippy::correctness)]

use std::error::Error;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

use midir::os::unix::VirtualOutput;
use midir::{Ignore, MidiIO, MidiInput, MidiOutput};
use parking_lot::Mutex;

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LoopState {
    #[default]
    Idle,
    Recording(Instant),
    Playing(Instant, Duration),
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut midi_in = MidiInput::new("Bloop.rs Input")?;
    midi_in.ignore(Ignore::None);
    let midi_out = MidiOutput::new("Bloop.rs Output")?;

    let in_port = select_port(&midi_in, "input")?;

    println!("\nOpening connections");

    let (tx, rx) = flume::bounded(1);
    let (midi_out_tx, midi_out_rx) = flume::bounded::<Box<[u8]>>(20);

    let loop_state: Arc<Mutex<LoopState>> = Arc::new(Mutex::new(LoopState::Idle));
    let loop1_buffer: Arc<Mutex<Vec<(Duration, Box<[u8]>)>>> = Arc::new(Mutex::new(vec![]));

    let mut conn_out = midi_out.create_virtual("Bloop.rs")?;

    let loop_state_ref = Arc::clone(&loop_state);
    let loop1_buffer_ref = Arc::clone(&loop1_buffer);
    let midi_out_tx_ref = midi_out_tx.clone();
    let _conn_in = midi_in.connect(
        &in_port,
        "blooprs-forward",
        move |_stamp, message, ()| {
            midi_out_tx_ref.send(message.into()).expect("channel error");
            if let LoopState::Recording(start) = *loop_state_ref.lock() {
                loop1_buffer_ref
                    .lock()
                    .push((Instant::now() - start, message.into()));
            }
        },
        (),
    )?;

    let loop_state_ref = Arc::clone(&loop_state);
    let loop1_buffer_ref = Arc::clone(&loop1_buffer);
    let midi_out_tx_ref = midi_out_tx.clone();
    std::thread::spawn(move || loop {
        let () = rx.recv().expect("channel error");
        let playing_loop_state = *loop_state_ref.lock();
        if let LoopState::Playing(mut start, loop_duration) = playing_loop_state {
            let buffer = loop1_buffer_ref.lock().clone();
            'buffer_loop: loop {
                for (offset, message) in &buffer {
                    sleep_until(start + *offset);
                    if *loop_state_ref.lock() != playing_loop_state {
                        break 'buffer_loop;
                    }
                    midi_out_tx_ref
                        .send(message.clone())
                        .expect("error sending");
                }
                start += loop_duration;
            }
        }
    });

    std::thread::spawn(move || {
        for message in midi_out_rx {
            conn_out.send(&message).expect("midi output error");
        }
    });

    std::thread::spawn(move || {});

    ncurses::initscr();
    loop {
        match ncurses::getch().try_into() {
            Ok(b'q') => break,
            Ok(b' ') => {
                let now = Instant::now();
                let mut state = loop_state.lock();
                *state = match *state {
                    LoopState::Idle => {
                        println!("Starting recording ...\r");
                        LoopState::Recording(now)
                    }
                    LoopState::Recording(start) => {
                        tx.send(()).expect("channel error");
                        let loop_duration = now - start;
                        println!("Recorded {loop_duration:?} loop. Now playing ...\r");
                        LoopState::Playing(now, loop_duration)
                    }
                    LoopState::Playing(_, _) => {
                        loop1_buffer.lock().clear();
                        println!("Cleared loop.\r");
                        LoopState::Idle
                    }
                }
            }
            _ => (),
        }
    }
    ncurses::endwin();

    Ok(())
}

fn select_port<T: MidiIO>(midi_io: &T, descr: &str) -> Result<T::Port, Box<dyn Error>> {
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
        .ok_or("Invalid port number")?;
    Ok(port.clone())
}

fn sleep_until(wake_time: std::time::Instant) {
    spin_sleep::sleep(wake_time - std::time::Instant::now());
}
