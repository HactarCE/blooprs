struct LuaNoteTracker {
    notes_pressed: HashSet<Note>,
    on_hook: LuaHook,
    off_hook: LuaHook,
}

struct LuaCCTracker {
    cc_values: HashMap<u7, u7>,
    hook: LuaHook,
}
