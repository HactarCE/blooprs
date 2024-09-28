time.mark_power_of_2()
Time { measure = ..., beats = ... }

hook()
    unhook()
    send(ev)
    clear_queue() -- can add param to filter which events to clear


display(ui)
    ui:label(string)


always shallowcopy table when sending


immutable userdata Time with fake properties: {
    global: 0.0, -- time since epoch (seconds, f64)
    measure: 0, -- integer, zero-indexed
    beat: 0, -- f64, zero-indexed
}

immutable userdata Duration with fake properties: {
    seconds: 0.0, -- seconds
    measures: 0, -- integer, zero-indexed
    beats: 0, -- f64, zero-indexed
}
