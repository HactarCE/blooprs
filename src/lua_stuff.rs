use std::sync::atomic::Ordering::Relaxed;
use std::{collections::BinaryHeap, sync::atomic::AtomicU64, time::Instant};

use eyre::Context;
use midly::num::{u4, u7};
use midly::PitchBend;
use midly::{live::LiveEvent, MidiMessage};
use rlua::prelude::*;

const EVENT_LIMIT: usize = 1000;

static NEXT_EVENT_ID: AtomicU64 = AtomicU64::new(0);

pub enum Command {
    MidiEvent(LiveEvent<'static>),
    RefreshUi,
}

pub struct LuaThread<'lua> {
    lua: LuaContext<'lua>,

    scripts: Vec<UserScript<'lua>>,

    event_queue: BinaryHeap<OrderedTimedEvent<'lua>>,
}
impl<'lua> LuaThread<'lua> {
    pub fn new(lua: LuaContext<'lua>) -> Self {
        lua.globals().set(
            "time",
            lua.create_table_from([("mark_power_of_2", lua.create_function(func))]),
        );

        Self {
            lua,

            scripts: vec![],

            event_queue: BinaryHeap::new(),
        }
    }

    pub fn run(
        mut self,
        commands_rx: flume::Receiver<Command>,
        midi_output_tx: flume::Sender<LiveEvent<'_>>,
        ui_state_output_tx: flume::Sender<LiveEvent<'_>>,
    ) -> () {
        loop {
            let next_event_time = self.do_events_and_return_wake_time();

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
                Command::MidiEvent(event) => match event {
                    LiveEvent::Midi { channel, message } => {
                        if let Ok(e) = Event::from_midi_message(self.lua, channel, message) {
                            self.send(e);
                        }
                    }
                    LiveEvent::Common(_) | LiveEvent::Realtime(_) => (),
                },

                Command::RefreshUi => {
                    todo!()
                }
            }
        }
    }

    fn load_file(&mut self, filename: &str, contents: &str) -> LuaResult<UserScript> {
        self.lua.load(contents).set_name(filename)?.exec()?;

        Ok(UserScript {
            filename: filename.to_owned(),
            event_hooks: vec![],
            ui_hooks: todo!(),
        })
    }

    fn do_events_and_return_wake_time(&mut self) -> Option<Instant> {
        let now = Instant::now();

        for _ in 0..EVENT_LIMIT {
            let OrderedTimedEvent(event) = self.event_queue.pop()?;
            let event_time = event.time;
            if event_time.global > now {
                // Oops! Too soon
                self.send(event);
                break;
            }
        }

        // Return the time of the next event.
        Some(self.event_queue.peek()?.0.time.global)
    }

    fn send(&mut self, event: Event<'lua>) {
        self.event_queue.push(OrderedTimedEvent(event))
    }
}

pub struct UserScript<'lua> {
    filename: String,
    event_hooks: Vec<EventHook<'lua>>,
    ui_hooks: Vec<UiHook<'lua>>,
}

#[derive(Debug)]
pub struct EventHook<'lua> {
    filter: Option<LuaTable<'lua>>,
    callback: LuaFunction<'lua>,
}

pub struct UiHook<'lua> {
    callback: LuaFunction<'lua>,
}

pub struct Event<'lua> {
    time: Time,
    order: Vec<u64>,
    user_data: LuaTable<'lua>,
    next_hook_index: usize,
}
impl<'lua> Event<'lua> {
    pub fn new(user_data: LuaTable<'lua>) -> Self {
        Self {
            time: Time {
                global: Instant::now(),
            },
            order: vec![NEXT_EVENT_ID.fetch_add(1, Relaxed)],
            user_data,
            next_hook_index: 0,
        }
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

struct OrderedTimedEvent<'lua>(Event<'lua>);
impl PartialEq for OrderedTimedEvent<'_> {
    fn eq(&self, other: &Self) -> bool {
        (self.0.time, &self.0.order) == (other.0.time, &other.0.order)
    }
}
impl Eq for OrderedTimedEvent<'_> {}
impl PartialOrd for OrderedTimedEvent<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(&other))
    }
}
impl Ord for OrderedTimedEvent<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse comparison to turn the maxheap into a minheap.
        (other.0.time, &other.0.order).cmp(&(self.0.time, &self.0.order))
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
