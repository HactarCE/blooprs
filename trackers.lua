function note_tracker()
  -- For each note, record whether it is pressed.
  local tracker = {
    enabled = true,
    pressed_notes = {},
  }

  hook({ on = true }, function(ev)
    -- Always retransmit event.
    send(ev)

    -- Only record note press if tracker is enabled.
    if tracker.enabled then
      tracker.pressed_notes[ev.note] = true
    end
  end)

  -- Record note releases and filter them so we don't send duplicates.
  hook({ off = true }, function(ev)
    -- Always retransmit event and record note release.
    send(ev)
    tracker.pressed_notes[ev.note] = nil
  end)

  function tracker:copy_state_from(other)
    for k, v in other.pressed_notes do
      self.pressed_notes[k] = v
    end
  end

  return tracker
end

function cc_tracker()
  local values = {}
  setmetatable(values, { __index = 0 })

  hook({ cc = true }, function(ev)
    send(ev)
    values[ev.cc] = ev.value
  end)

  return values
end

function velocity_tracker()
  local note_velocities = {}

  hook({ on = true }, function(ev)
    send(ev)
    note_velocities[ev.note] = ev.vel
  end)

  return note_velocities
end
