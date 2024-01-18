use midly::num::u7;
use midly::MidiMessage;

pub enum KeyEffect {
    Press { key: u7, vel: u7 },
    Release { key: u7 },
    Aftertouch { key: u7 },
    None,
}
impl From<MidiMessage> for KeyEffect {
    fn from(message: MidiMessage) -> Self {
        match message {
            MidiMessage::NoteOff { key, vel: _ } => KeyEffect::Release { key },
            MidiMessage::NoteOn { key, vel } if vel == 0 => KeyEffect::Release { key },
            MidiMessage::NoteOn { key, vel } => KeyEffect::Press { key, vel },
            MidiMessage::Aftertouch { key, vel: _ } => KeyEffect::Aftertouch { key },
            _ => KeyEffect::None,
        }
    }
}
