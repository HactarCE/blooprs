use std::{
    collections::BinaryHeap,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use midly::{
    num::{u4, u7},
    MidiMessage, PitchBend,
};
use rlua::prelude::*;

#[derive(Debug, Clone)]
pub struct Event<'lua> {
    user_data: LuaTable<'lua>,
}
impl<'lua> FromLua<'lua> for Event<'lua> {
    fn from_lua(lua_value: LuaValue<'lua>, lua: LuaContext<'lua>) -> LuaResult<Self> {
        let user_data = <LuaTable<'lua>>::from_lua(lua_value, lua)?;
        Ok(Event { user_data })
    }
}
impl<'lua> Event<'lua> {
    pub fn new(user_data: LuaTable<'lua>) -> Self {
        Self { user_data }
    }
    pub fn from_iter<'a>(
        lua: LuaContext<'lua>,
        kv_pairs: impl IntoIterator<Item = (&'a str, LuaValue<'lua>)>,
    ) -> LuaResult<Self> {
        Ok(Self::new(lua.create_table_from(kv_pairs)?))
    }
    pub fn new_key_event(
        lua: LuaContext<'lua>,
        kind: &str,
        channel: u4,
        key: u7,
        vel: u7,
    ) -> LuaResult<Self> {
        Self::from_iter(
            lua,
            [
                (kind, LuaValue::Boolean(true)),
                ("ch", LuaValue::Integer(channel.as_int() as i64)),
                ("key", LuaValue::Integer(key.as_int() as i64)),
                ("vel", u7_to_lua_float(vel)),
            ],
        )
    }

    pub fn from_midi_message(
        lua: LuaContext<'lua>,
        channel: u4,
        midi_message: MidiMessage,
    ) -> LuaResult<Self> {
        let mut kv_pairs = match midi_message {
            MidiMessage::NoteOff { key, vel } => vec![
                ("off", LuaValue::Boolean(true)),
                ("key", LuaValue::Integer(key.as_int() as i64)),
                ("vel", u7_to_lua_float(vel)),
            ],
            MidiMessage::NoteOn { key, vel } if vel == 0 => vec![
                ("off", LuaValue::Boolean(true)),
                ("key", LuaValue::Integer(key.as_int() as i64)),
                ("vel", LuaValue::Integer(0)),
            ],
            MidiMessage::NoteOn { key, vel } => vec![
                ("on", LuaValue::Boolean(true)),
                ("key", LuaValue::Integer(key.as_int() as i64)),
                ("vel", u7_to_lua_float(vel)),
            ],
            MidiMessage::Aftertouch { key, vel } => vec![
                ("aftertouch", LuaValue::Boolean(true)),
                ("key", LuaValue::Integer(key.as_int() as i64)),
                ("vel", u7_to_lua_float(vel)),
            ],
            MidiMessage::Controller { controller, value } => vec![
                ("cc", LuaValue::Integer(controller.as_int() as i64)),
                ("value", u7_to_lua_float(value)),
            ],
            MidiMessage::ProgramChange { program } => vec![
                ("ch", LuaValue::Integer(channel.as_int() as i64)),
                ("prog", LuaValue::Integer(program.as_int() as i64)),
            ],
            MidiMessage::ChannelAftertouch { vel } => vec![
                ("ch", LuaValue::Integer(channel.as_int() as i64)),
                ("aftertouch", LuaValue::Boolean(true)),
                ("vel", LuaValue::Integer(vel.as_int() as i64)),
            ],
            MidiMessage::PitchBend { bend } => vec![
                ("ch", LuaValue::Integer(channel.as_int() as i64)),
                ("bend", LuaValue::Number(bend.as_f64())),
            ],
        };

        kv_pairs.push(("ch", LuaValue::Integer(channel.as_int() as i64)));

        Ok(Self::new(lua.create_table_from(kv_pairs)?))
    }
}

pub type TimedEventHeap<'lua> = BinaryHeap<TimedEvent<'lua>>;

static NEXT_EVENT_ORDER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct TimedEvent<'lua> {
    pub time: Time,
    pub order: u64,
    pub event: Event<'lua>,
}
impl PartialEq for TimedEvent<'_> {
    fn eq(&self, other: &Self) -> bool {
        (self.time, self.order) == (other.time, other.order)
    }
}
impl Eq for TimedEvent<'_> {}
impl PartialOrd for TimedEvent<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(&other))
    }
}
impl Ord for TimedEvent<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse comparison to turn the maxheap into a minheap.
        (other.time, other.order).cmp(&(self.time, self.order))
    }
}
impl<'lua> TimedEvent<'lua> {
    pub fn new(time: Time, event: Event<'lua>) -> Self {
        let order = NEXT_EVENT_ORDER.fetch_add(1, Ordering::Relaxed);
        Self { time, order, event }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Time {
    global: Instant,
}

fn u7_to_lua_float<'lua>(x: u7) -> LuaValue<'lua> {
    LuaValue::Number(x.as_int() as LuaNumber / u7::max_value().as_int() as LuaNumber)
}

fn pitch_bend_to_lua_float<'lua>(bend: PitchBend) -> LuaValue<'lua> {
    LuaValue::Number(bend.as_f64())
}
