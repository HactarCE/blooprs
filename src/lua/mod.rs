use itertools::Itertools;
use lazy_static::lazy_static;
use std::path::PathBuf;

use rlua::prelude::*;

use event::{Event, Time, TimedEvent, TimedEventHeap};

mod event;
mod prelude;

lazy_static! {
    #[rustfmt::skip]
    static ref LUA_PATH: PathBuf =
        std::env::current_exe().unwrap()
            .canonicalize().unwrap()
            .parent().unwrap()
            .parent().unwrap()
            .parent().unwrap()
            .to_owned()
            .join("lua");
}

pub struct LuaState<'lua> {
    lua: LuaContext<'lua>,

    hooks: Vec<LuaHook<'lua>>,

    hooks_owned_by_file:
}

impl<'lua> LuaState<'lua> {
    pub fn new(lua: LuaContext<'lua>) -> LuaResult<Self> {
        lua.globals()
            .set("require", lua.create_function(lua_require)?)?;

        Ok(Self { lua, hooks: vec![] })
    }

    pub fn load_file(&mut self, filename: &str) -> LuaResult<FileLoadResult> {
        self.run_lua(|this| lua_require(this.lua, filename.to_string()));
    }

    pub fn run_lua(
        &mut self,
        f: impl FnOnce(&mut Self) -> LuaResult<()>,
    ) -> LuaResult<RunResult<'lua>> {
        let g = self.lua.globals();

        g.set("ADDED_HOOKS", self.lua.create_table()?)?;
        g.set("REMOVED_HOOKS", self.lua.create_table()?)?;
        g.set("EVENTS_TO_SEND", self.lua.create_table()?)?;
        g.set("CLEAR_QUEUE", self.lua.create_table()?)?;

        f(self)?;

        fn get_global_seq_table<'lua, V: FromLua<'lua>>(
            g: &LuaTable<'lua>,
            key: &str,
        ) -> LuaResult<Vec<V>> {
            g.get::<_, LuaTable<'lua>>(key)?
                .sequence_values()
                .try_collect()
        }

        Ok(RunResult {
            added_hooks: get_global_seq_table(&g, "ADDED_HOOKS")?,
            removed_hooks: get_global_seq_table(&g, "REMOVED_HOOKS")?,
            events_to_send: get_global_seq_table(&g, "EVENTS_TO_SEND")?,
            clear_queue: get_global_seq_table(&g, "CLEAR_QUEUE")?,
        })
    }
}

fn lua_require<'lua>(lua: LuaContext<'lua>, mut filename: String) -> LuaResult<LuaValue<'lua>> {
    if !filename.ends_with(".lua") {
        filename += ".lua";
    }

    let mut path = LUA_PATH.clone();
    path.extend(filename.split('/'));
    let file_contents = std::fs::read_to_string(path).map_err(|e| LuaError::external(e))?;

    let sandbox_env: LuaTable<'_> = lua
        .globals()
        .get::<_, LuaFunction>("make_sandbox")?
        .call(())?;

    lua.load(&file_contents)
        .set_name(&filename)?
        .set_environment(sandbox_env)?
        .eval()
}

#[derive(Debug, Clone)]
struct LuaHook<'lua> {
    id: u32,
    filter: Option<LuaTable<'lua>>,
    callback: LuaFunction<'lua>,
    event_queue: TimedEventHeap<'lua>,
}
impl<'lua> FromLua<'lua> for LuaHook<'lua> {
    fn from_lua(lua_value: LuaValue<'lua>, lua: LuaContext<'lua>) -> LuaResult<Self> {
        let table = LuaTable::from_lua(lua_value, lua)?;
        Ok(Self {
            id: table.get("id")?,
            filter: table.get("filter")?,
            callback: table.get("callback")?,
            event_queue: TimedEventHeap::new(),
        })
    }
}
impl<'lua> LuaHook<'lua> {
    pub fn queue_event(&mut self, time: Time, event: Event<'lua>) {
        self.event_queue.push(TimedEvent::new(time, event))
    }
}

struct RunResult<'lua> {
    added_hooks: Vec<LuaHook<'lua>>,
    removed_hooks: Vec<u32>,
    events_to_send: Vec<Event<'lua>>,
    clear_queue: Vec<LuaTable<'lua>>,
}
