use std::time::{Duration, Instant};

use eyre::Result;
use itertools::Itertools;
use midly::live::LiveEvent;
use midly::num::{u4, u7};
use midly::MidiMessage;

use crate::key_effect::KeyEffect;
use crate::key_tracker::{ChannelSet, KeySet, KeyStatus, PerKey};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct TimedMidiMessage {
    pub time: Instant,
    pub message: MidiMessage,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BloopPlayback {
    /// Keys currently pressed by this playback.
    keys_pressed: KeySet,
    /// Index into the recording buffer of the next event to play back.
    index: usize,
    /// Time offset compared to the recording of the buffer.
    offset: Duration,
}
impl BloopPlayback {
    pub fn new(offset: Duration) -> Self {
        Self {
            keys_pressed: KeySet::new(),
            index: 0,
            offset,
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct MidiPassThrough {
    keys: PerKey<ChannelSet>,
    is_listening: bool,
}
impl MidiPassThrough {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_listening(is_listening: bool) -> Self {
        Self {
            keys: PerKey::default(),
            is_listening,
        }
    }
    pub fn filter_midi(&mut self, channel: u4, message: MidiMessage) -> bool {
        match KeyEffect::from(message) {
            // Allow note-on events iff we're listening.
            KeyEffect::Press { key, vel: _ } if self.is_listening => {
                self.keys[key].set_on(channel);
                true
            }

            // Allow note-off events only if the key is no longer held by the
            // user.
            KeyEffect::Release { key } => {
                self.keys[key].set_off(channel);
                !self.keys[key].any()
            }

            // Allow polyphonic aftertouch only if the key is held.
            KeyEffect::Aftertouch { key } => self.keys[key].any(),

            // Allow other events iff we're listening.
            _ => self.is_listening,
        }
    }
}

pub struct Bloop {
    /// MIDI output channel.
    midi_out_tx: flume::Sender<LiveEvent<'static>>,
    /// User configuration.
    config: BloopConfig,

    /// State of MIDI passthrough (MIDI input -> output).
    passthru: MidiPassThrough,
    /// State of MIDI recording (MIDI input -> loop buffer).
    recorder: MidiPassThrough,
    /// Whether playback should make sound (loop buffer -> output).
    is_playback_active: bool,

    /// Input and output keys state.
    keys: PerKey<KeyStatus>,

    /// Buffer of recorded MIDI messages.
    recording_buffer: Vec<TimedMidiMessage>,

    /// Keys held at the start of the recording, with their corresponding
    /// velocities.
    recording_start_state: Vec<(u7, u7)>,
    /// Keys held at the end of the recording.
    recording_end_state: KeySet,

    /// Start time of recording. When recording or playing, this must be `Some`.
    recording_start_time: Option<Instant>,
    /// End time of recording. When recording, this may be `Some`. When playing,
    /// this must be `Some`.
    recording_end_time: Option<Instant>,

    /// Playbacks in progress.
    playbacks: Vec<BloopPlayback>,
    /// Next playback offset.
    next_queued_playback_time: Option<Instant>,
}

impl Bloop {
    pub fn new(midi_out_tx: flume::Sender<LiveEvent<'static>>, output_channel: u4) -> Self {
        Self {
            midi_out_tx,
            config: BloopConfig { output_channel },

            passthru: MidiPassThrough::with_listening(true),
            recorder: MidiPassThrough::new(),
            is_playback_active: true,

            keys: PerKey::default(),

            recording_buffer: vec![],
            recording_start_state: vec![],
            recording_end_state: KeySet::new(),
            recording_start_time: None,
            recording_end_time: None,

            playbacks: vec![],
            next_queued_playback_time: None,
        }
    }

    /// Returns whether a key is held by the user or by any playback of the
    /// loop.
    fn is_key_held(&self, key: u7) -> bool {
        self.keys[key].input.any()
            || (self.is_playback_active
                && self
                    .playbacks
                    .iter()
                    .any(|playback| playback.keys_pressed.contains(key)))
    }

    /// Sends a MIDI message.
    ///
    /// Ignores note-off events for keys that should remain held.
    fn send(&self, message: MidiMessage) {
        // If something else is keeping the key held, don't release it yet.
        match KeyEffect::from(message) {
            KeyEffect::Release { key, .. } if self.is_key_held(key) => return,
            _ => (),
        }

        let channel = self.config.output_channel;
        let event = LiveEvent::Midi { channel, message };
        if let Err(e) = self.midi_out_tx.send(event) {
            log::error!("Error sending MIDI event: {e}");
        }
    }

    pub fn playback_keys_pressed(&self) -> KeySet {
        self.playbacks
            .iter()
            .map(|playback| playback.keys_pressed)
            .fold(KeySet::new(), |a, b| a | b)
    }
    pub fn release_keys(&self, keys_to_release: KeySet) {
        for key in keys_to_release.iter_keys() {
            self.send(MidiMessage::NoteOn { key, vel: 0.into() });
        }
    }

    /// Cancels all in-progress playbacks of the loop.
    pub fn cancel_recording(&mut self) {
        if self.recording_start_time.is_some() {
            self.recording_start_time = None;
            self.recording_end_time = None;
            self.recorder.is_listening = false;
        }
    }
    pub fn cancel_all_playbacks(&mut self) {
        let keys_to_release = self.playback_keys_pressed();
        self.playbacks.clear();
        self.cancel_next_playback();
        self.release_keys(keys_to_release);
    }
    pub fn cancel_next_playback(&mut self) {
        self.next_queued_playback_time = None;
    }
    pub fn is_recording(&self) -> bool {
        let now = Instant::now();
        let past_start = self
            .recording_start_time
            .is_some_and(|start_time| start_time <= now);
        let past_end = self
            .recording_end_time
            .is_some_and(|end_time| end_time <= now);
        past_start && !past_end
    }
    pub fn toggle_listening(&mut self) {
        self.passthru.is_listening = !self.passthru.is_listening;
        if self.is_recording() {
            self.recorder.is_listening = self.passthru.is_listening;
        }
    }
    pub fn toggle_playing(&mut self) {
        self.is_playback_active = !self.is_playback_active;
        if self.is_playback_active {
            // Press keys that should be held.
            for key in self.playback_keys_pressed().iter_keys() {
                // Is the user helding the key already?
                if !self.keys[key].input.any() {
                    // The user is not holding the key, so we should press it.
                    let vel = self.keys[key].last_velocity;
                    self.send(MidiMessage::NoteOn { key, vel });
                }
            }
        } else {
            // Release keys that should not be pressed.
            self.release_keys(self.playback_keys_pressed());
        }
    }
    pub fn start_recording(&mut self, start: Instant, end: Option<Instant>) {
        self.recording_start_time = Some(start);
        self.recording_end_time = end;
    }
    pub fn start_playing(&mut self, duration: Duration) {
        log::trace!("Start playing");

        self.recorder.is_listening = false;

        self.recording_end_state = self
            .keys
            .iter()
            .map(|(_, status)| status.input.any())
            .collect();

        let Some(start_time) = self.recording_start_time else {
            log::error!("cannot start playing with no start time");
            return;
        };
        self.recording_end_time = Some(start_time + duration);

        self.next_queued_playback_time = self.recording_end_time;
    }

    pub fn recv_midi(&mut self, channel: u4, event: TimedMidiMessage) {
        if self.passthru.filter_midi(channel, event.message) {
            match KeyEffect::from(event.message) {
                KeyEffect::Press { key, vel } => {
                    self.keys[key].input.set_on(channel);
                    self.keys[key].last_velocity = vel;
                }
                KeyEffect::Release { key } => self.keys[key].input.set_off(channel),
                KeyEffect::Aftertouch { .. } | KeyEffect::None => (),
            }
            self.send(event.message);
        }

        if self.recorder.filter_midi(channel, event.message) {
            match KeyEffect::from(event.message) {
                KeyEffect::Press { key, vel } => {
                    self.keys[key].recording.set_on(channel);
                    self.keys[key].last_velocity = vel;
                }
                KeyEffect::Release { key } => self.keys[key].recording.set_off(channel),
                KeyEffect::Aftertouch { .. } | KeyEffect::None => (),
            }
            self.recording_buffer.push(event);
        }
    }

    pub fn do_events_and_return_wake_time(&mut self, now: Instant) -> Option<Instant> {
        let start_time = self.recording_start_time?;

        if now <= start_time {
            // We are not ready to start recording.
            return Some(start_time);
        }

        if self.is_recording() && !self.recorder.is_listening {
            // Start recording!
            log::trace!("Start recording");
            self.recorder.is_listening = self.passthru.is_listening;
            self.recording_buffer.clear();
            self.recording_start_state = self
                .keys
                .iter()
                .filter(|(_, status)| status.input.any())
                .map(|(i, status)| (i, status.last_velocity))
                .collect_vec();
        }

        let end_time = self.recording_end_time?;
        let loop_duration = end_time - start_time;

        if self.recorder.is_listening {
            if now <= end_time {
                // We are not ready to stop recording. Keep recording.
                return Some(end_time);
            } else {
                // Stop recording and start playing!
                self.start_playing(loop_duration);
            }
        }

        if let Some(queued_playback_time) = self.next_queued_playback_time {
            if queued_playback_time <= now {
                log::trace!("Starting new playback");
                self.next_queued_playback_time = None;

                // Catch up to the present, to avoid duplicate note-on events.
                self.do_events_and_return_wake_time(queued_playback_time);

                // Press any notes that should be pressed at the start of
                // playback and aren't already.
                let mut playback = BloopPlayback::new(queued_playback_time - start_time);
                for &(key, vel) in &self.recording_start_state {
                    playback.keys_pressed.insert(key);
                    if self.is_playback_active {
                        self.send(MidiMessage::NoteOn { key, vel });
                    }
                }
                // Start the playback.
                self.playbacks.push(playback);

                // Queue the next playback.
                log::trace!("Queueing next playback");
                self.next_queued_playback_time = Some(queued_playback_time + loop_duration);
            }
        }

        let mut wake_time = self.next_queued_playback_time;
        let mut queued_events = vec![];

        self.playbacks.retain_mut(|playback| {
            while let Some(event) = self.recording_buffer.get(playback.index) {
                if event.time + playback.offset > now {
                    // Wake at the next event.
                    wake_time = Some(option_at_most(wake_time, event.time + playback.offset));
                    // Keep this playback.
                    return true;
                }

                // Simulate this event.
                playback.keys_pressed.update(event.message);
                if let KeyEffect::Press { key, vel } = event.message.into() {
                    self.keys[key].last_velocity = vel;
                }
                // Send this event.
                if self.is_playback_active {
                    queued_events.push(event);
                }

                // Play the next event.
                playback.index += 1;
            }
            false // End this playback.
        });

        queued_events.sort_by_key(|event| event.time);
        for event in queued_events {
            self.send(event.message);
        }

        wake_time
    }

    fn ui_state(&self) -> BloopUiState {
        BloopUiState {
            is_listening: self.passthru.is_listening,
            is_waiting_to_record: self
                .recording_start_time
                .is_some_and(|start_time| start_time > Instant::now()),
            is_recording: self.is_recording(),
            is_playing_back: !self.playbacks.is_empty() || self.next_queued_playback_time.is_some(),
            is_playback_active: self.is_playback_active,
        }
    }
}

pub struct BloopConfig {
    output_channel: u4,
}

pub enum BloopCommand {
    RefreshUi,

    Midi(LiveEvent<'static>),

    DoKey(usize),
    ToggleListening(usize),
    TogglePlayback(usize),
    CancelPlaying(usize),
    StartRecording(usize),
    StartPlaying(usize),
    ClearAll,
}
impl From<LiveEvent<'static>> for BloopCommand {
    fn from(value: LiveEvent<'static>) -> Self {
        BloopCommand::Midi(value)
    }
}

pub struct UiState {
    pub epoch: Option<Instant>,
    pub duration: Option<Duration>,
    pub bloops: Vec<BloopUiState>,
}

pub struct BloopUiState {
    pub is_listening: bool,
    pub is_waiting_to_record: bool,
    pub is_recording: bool,
    pub is_playing_back: bool,
    pub is_playback_active: bool,
}

pub fn spawn_bloops_thread(
    commands_tx: flume::Sender<BloopCommand>,
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
                .filter_map(|b| b.do_events_and_return_wake_time(Instant::now()))
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

                BloopCommand::Midi(LiveEvent::Midi { channel, message }) => {
                    let time = Instant::now();
                    let message = TimedMidiMessage { time, message };
                    if let KeyEffect::Press { key, vel: _ } = KeyEffect::from(message.message) {
                        match (channel.as_int(), key.as_int()) {
                            (4, 76) => commands_tx.send(BloopCommand::ClearAll).unwrap(),
                            (5, 77) => commands_tx.send(BloopCommand::DoKey(0)).unwrap(),
                            (4, 78) => bloops[0].toggle_listening(),
                            (5, 79) => commands_tx.send(BloopCommand::DoKey(1)).unwrap(),
                            (4, 80) => bloops[1].toggle_listening(),
                            (5, 81) => commands_tx.send(BloopCommand::DoKey(2)).unwrap(),
                            (4, 82) => bloops[2].toggle_listening(),
                            _ => {
                                for bloop in &mut bloops {
                                    bloop.recv_midi(channel, message);
                                }
                            }
                        }
                    } else {
                        for bloop in &mut bloops {
                            bloop.recv_midi(channel, message);
                        }
                    }
                }
                BloopCommand::Midi(_) => (), // Ignore other MIDI events

                BloopCommand::DoKey(i) => {
                    if bloops[i].is_recording() {
                        commands_tx.send(BloopCommand::StartPlaying(i)).unwrap();
                    } else if !bloops[i].playbacks.is_empty()
                        || bloops[i].next_queued_playback_time.is_some()
                    {
                        commands_tx.send(BloopCommand::TogglePlayback(i)).unwrap();
                    } else {
                        commands_tx.send(BloopCommand::StartRecording(i)).unwrap();
                    }
                }
                BloopCommand::ToggleListening(i) => bloops[i].toggle_listening(),
                BloopCommand::TogglePlayback(i) => bloops[i].toggle_playing(),
                BloopCommand::CancelPlaying(i) => bloops[i].cancel_all_playbacks(),
                BloopCommand::StartRecording(i) => {
                    if epoch.is_none() || duration.is_none() {
                        // If we don't know the tempo, then stop recording on
                        // another bloop and use that to infer the tempo.
                        if let Some(recording_bloop) =
                            bloops.iter_mut().find(|bloop| bloop.recorder.is_listening)
                        {
                            if let Some(start) = recording_bloop.recording_start_time {
                                let end = Instant::now();
                                epoch = Some(start);
                                duration = Some(end - start);
                                recording_bloop.start_playing(end - start);
                            }
                        }
                    }

                    if let Some((next_start, next_end)) = next_loop_time(epoch, duration) {
                        log::trace!(
                            "Schedule recording start on #{i} in {:?}",
                            next_start - Instant::now(),
                        );
                        bloops[i].start_recording(next_start, Some(next_end));
                    } else {
                        log::trace!("Schedule recording start on #{i}");
                        bloops[i].start_recording(Instant::now(), None);
                    }
                }
                BloopCommand::StartPlaying(i) => {
                    if epoch.is_some() || duration.is_some() {
                        continue; // We already know the tempo, so ignore this request.
                    }
                    if let Some(start) = bloops[i].recording_start_time {
                        let end = Instant::now();
                        epoch = Some(start);
                        duration = Some(end - start);
                        bloops[i].start_playing(end - start);
                    }
                }
                BloopCommand::ClearAll => {
                    for bloop in &mut bloops {
                        bloop.cancel_recording();
                        bloop.cancel_all_playbacks();
                    }
                    epoch = None;
                    duration = None;
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

pub fn option_at_most<T: PartialOrd>(a: Option<T>, b: T) -> T {
    match a {
        Some(a) if a < b => a,
        _ => b,
    }
}
