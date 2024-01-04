use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use eyre::Result;
use itertools::Itertools;
use midly::live::LiveEvent;
use midly::num::{u4, u7};
use midly::MidiMessage;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct TimedMidiMessage {
    pub time: Instant,
    pub message: MidiMessage,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BloopPlayback {
    /// Index into the recording buffer of the next event to play back.
    index: usize,
    /// Time offset compared to the recording of the buffer.
    offset: Duration,
    /// Duration after which to restart playback.
    loop_duration: Option<Duration>,
}
impl BloopPlayback {
    pub fn new_loop(duration: Duration) -> Self {
        Self {
            index: 0,
            offset: duration,
            loop_duration: Some(duration),
        }
    }
    pub fn next(self) -> Option<Self> {
        Some(Self {
            index: 0,
            offset: self.offset + self.loop_duration?,
            loop_duration: self.loop_duration,
        })
    }
}

pub struct PressedKeys(HashMap<u7, usize>);
impl PressedKeys {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
    pub fn drain(&mut self) -> impl IntoIterator<Item = u7> {
        std::mem::take(&mut self.0).into_keys()
    }
    pub fn note_on(&mut self, key: u7) -> bool {
        let multiplicity = self.0.entry(key).or_default();
        *multiplicity += 1;
        *multiplicity == 1
    }
    pub fn note_off(&mut self, key: u7) -> bool {
        match self.0.get_mut(&key) {
            Some(1) => {
                self.0.remove(&key);
                true
            }
            Some(multiplicity) => {
                *multiplicity -= 1;
                false
            }
            None => false,
        }
    }
    pub fn contains(&self, key: u7) -> bool {
        self.0.contains_key(&key)
    }
}

pub struct Bloop {
    midi_out_tx: flume::Sender<LiveEvent<'static>>,
    config: BloopConfig,

    /// For each key: how many times has it been pressed. (Every note-on must
    /// have a matching note-off before we send a note-off event.)
    pressed_keys: PressedKeys,
    recording_buffer: VecDeque<TimedMidiMessage>,
    playbacks: Vec<BloopPlayback>,

    is_active: bool,
    state: BloopState,
}

impl Bloop {
    pub fn new(midi_out_tx: flume::Sender<LiveEvent<'static>>, output_channel: u4) -> Self {
        Self {
            midi_out_tx,
            config: BloopConfig { output_channel },

            pressed_keys: PressedKeys::new(),
            recording_buffer: VecDeque::new(),
            playbacks: vec![],

            is_active: false,
            state: BloopState::Idle,
        }
    }

    fn send(&self, message: MidiMessage) {
        let channel = self.config.output_channel;
        let event = LiveEvent::Midi { channel, message };
        if let Err(e) = self.midi_out_tx.send(event) {
            log::error!("Error sending MIDI event: {e}");
        }
    }

    pub fn activate(&mut self) {
        self.is_active = true;
    }
    pub fn deactivate(&mut self) {
        self.clear_notes();
        self.is_active = false;
    }
    pub fn clear_notes(&mut self) {
        if self.is_active {
            for key in self.pressed_keys.drain() {
                let vel = u7::max_value();
                self.send(MidiMessage::NoteOff { key, vel });
            }
        }
    }
    pub fn toggle_active(&mut self) {
        match self.is_active {
            true => self.deactivate(),
            false => self.activate(),
        }
    }
    pub fn start_recording(&mut self, start: Instant, end: Option<Instant>) {
        self.state = BloopState::Recording { start, end };
        self.clear_old_recorded_messages();
    }
    pub fn start_playing(&mut self, duration: Duration) {
        self.playbacks.push(BloopPlayback::new_loop(duration));
        self.state = BloopState::Playing;
    }
    pub fn clear(&mut self) {
        self.clear_notes();
        self.recording_buffer.clear();
        self.playbacks.clear();
        self.state = BloopState::Idle;
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.state, BloopState::Recording { .. })
    }
    fn recording_start(&self) -> Option<Instant> {
        match self.state {
            BloopState::Recording { start, .. } => Some(start),
            _ => None,
        }
    }
    fn clear_old_recorded_messages(&mut self) {
        while let Some(message) = self.recording_buffer.front() {
            let start = self.recording_start().unwrap_or_else(Instant::now);
            if message.time < start - crate::LOOKBACK_TIME {
                self.recording_buffer.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn recv_midi(&mut self, message: TimedMidiMessage) {
        // Ignore if we never saw the corresponding `NoteOn` event.
        match message.message {
            MidiMessage::NoteOff { key, .. } if !self.pressed_keys.note_off(key) => return,
            MidiMessage::Aftertouch { key, .. } if !self.pressed_keys.contains(key) => return,
            _ => (),
        }

        if self.is_active {
            if let MidiMessage::NoteOn { key, vel: _ } = message.message {
                if !self.pressed_keys.note_on(key) && !crate::ALLOW_UNMATCHED_NOTE_ON {
                    return;
                }
            }
            self.send(message.message);
        }

        match self.state {
            BloopState::Idle | BloopState::Recording { .. } => {
                self.clear_old_recorded_messages();
                self.recording_buffer.push_back(message);
            }
            BloopState::Playing { .. } => (),
        }
    }

    pub fn do_events_and_get_next_event_time(&mut self) -> Option<Instant> {
        match self.state {
            BloopState::Recording {
                start,
                end: Some(end),
            } => {
                if end >= Instant::now() {
                    self.start_playing(end - start);
                    None
                } else {
                    Some(end)
                }
            }

            BloopState::Playing => std::mem::take(&mut self.playbacks)
                .into_iter()
                .filter_map(|mut playback| {
                    while let Some(message) = self.recording_buffer.get(playback.index) {
                        if message.time + playback.offset > Instant::now() {
                            self.playbacks.push(playback);
                            return Some(message.time + playback.offset);
                        }
                        if self.is_active {
                            match message.message {
                                MidiMessage::NoteOn { key, .. } => {
                                    self.pressed_keys.note_on(key);
                                    self.send(message.message);
                                }
                                MidiMessage::NoteOff { key, .. } => {
                                    if self.pressed_keys.note_off(key) {
                                        self.send(message.message);
                                    }
                                }
                                MidiMessage::Aftertouch { key, .. } => {
                                    if self.pressed_keys.contains(key) {
                                        self.send(message.message);
                                    }
                                }
                                _ => self.send(message.message),
                            }
                        }
                        if playback.index == 0 {
                            self.playbacks.extend(playback.next());
                        }
                        playback.index += 1;
                    }
                    None
                })
                .min(),

            _ => None,
        }
    }

    fn ui_state(&self) -> BloopUiState {
        BloopUiState {
            is_active: self.is_active,
            state: self.state,
        }
    }
}

pub struct BloopConfig {
    output_channel: u4,
}

pub enum BloopCommand {
    RefreshUi,

    Midi(LiveEvent<'static>),

    ToggleActive(usize),
    StartRecording(usize),
    StartPlaying(usize),
    Clear(usize),
}
impl From<LiveEvent<'static>> for BloopCommand {
    fn from(value: LiveEvent<'static>) -> Self {
        BloopCommand::Midi(value)
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub enum BloopState {
    #[default]
    Idle,
    Recording {
        start: Instant,
        end: Option<Instant>,
    },
    Playing,
}

pub struct UiState {
    pub epoch: Option<Instant>,
    pub duration: Option<Duration>,
    pub bloops: Vec<BloopUiState>,
}

pub struct BloopUiState {
    pub is_active: bool,
    pub state: BloopState,
}

pub fn spawn_bloops_thread(
    commands_rx: flume::Receiver<BloopCommand>,
) -> Result<flume::Receiver<UiState>> {
    let midi_out_tx = crate::midi_out::spawn_midi_out_thread()?;
    let (ui_state_tx, ui_state_rx) = flume::bounded(1);

    std::thread::spawn(move || {
        let mut epoch = None;
        let mut duration = None;
        let mut bloops = vec![
            Bloop::new(midi_out_tx.clone(), 0.into()),
            Bloop::new(midi_out_tx.clone(), 1.into()),
            Bloop::new(midi_out_tx.clone(), 2.into()),
        ];

        loop {
            let next_event_time = bloops
                .iter_mut()
                .filter_map(|b| b.do_events_and_get_next_event_time())
                .min();

            let command = if let Some(deadline) = next_event_time {
                match commands_rx.recv_deadline(deadline) {
                    Ok(command) => command,
                    Err(flume::RecvTimeoutError::Disconnected) => return,
                    Err(flume::RecvTimeoutError::Timeout) => continue,
                }
            } else {
                match commands_rx.recv() {
                    Ok(command) => command,
                    Err(flume::RecvError::Disconnected) => return,
                }
            };

            match command {
                BloopCommand::RefreshUi => {
                    let ui_state = UiState {
                        epoch,
                        duration,
                        bloops: bloops.iter().map(|bloop| bloop.ui_state()).collect_vec(),
                    };
                    if ui_state_tx.send(ui_state).is_err() {
                        return;
                    }
                }

                BloopCommand::Midi(LiveEvent::Midi {
                    channel: _,
                    message,
                }) => {
                    let time = Instant::now();
                    let message = TimedMidiMessage { time, message };
                    for bloop in &mut bloops {
                        bloop.recv_midi(message.clone());
                    }
                }
                BloopCommand::Midi(_) => (), // Ignore other MIDI events

                BloopCommand::ToggleActive(i) => bloops[i].toggle_active(),
                BloopCommand::StartRecording(i) => {
                    if epoch.is_none() || duration.is_none() {
                        if let Some(recording_bloop) =
                            bloops.iter_mut().find(|bloop| bloop.is_recording())
                        {
                            let start = match recording_bloop.state {
                                BloopState::Recording { start, .. } => start,
                                _ => unreachable!(),
                            };
                            let end = Instant::now();
                            recording_bloop.start_playing(end - start);
                            epoch = Some(start);
                            duration = Some(end - start);
                        }
                    }

                    if let Some((next_start, next_end)) = next_loop_time(epoch, duration) {
                        bloops[i].start_recording(next_start, Some(next_end));
                    } else {
                        bloops[i].start_recording(Instant::now(), None);
                    }
                }
                BloopCommand::StartPlaying(i) => {
                    if epoch.is_some() || duration.is_some() {
                        continue; // ignore
                    }
                    let start = match bloops[i].state {
                        BloopState::Recording { start, .. } => start,
                        _ => continue, // ignore
                    };
                    let end = Instant::now();
                    bloops[i].start_playing(end - start);
                    epoch = Some(start);
                    duration = Some(end - start);
                }
                BloopCommand::Clear(i) => {
                    bloops[i].clear();
                    if bloops.iter().all(|bloop| bloop.state == BloopState::Idle) {
                        epoch = None;
                        duration = None;
                    }
                }
            }
        }
    });

    Ok(ui_state_rx)
}

fn next_loop_time(
    epoch: Option<Instant>,
    duration: Option<Duration>,
) -> Option<(Instant, Instant)> {
    let loops_elapsed = (Instant::now() - epoch?).as_secs_f32() / duration?.as_secs_f32();
    let next_start = epoch? + duration? * loops_elapsed.ceil() as u32;
    let next_end = next_start + duration?;
    Some((next_start, next_end))
}
