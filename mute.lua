-- Record the latest velocity of each key so that when we unmute we can press
-- them again with the same velocity.
local latest_note_velocities = {}
hook({ on = true }, function(ev)
  latest_note_velocities[ev.note] = ev.vel
end)

local input_tracker

-- Put this hook before we initialize the input tracker so that the input
-- tracker sees our note-off events.
hook({ cancel = true }, function(ev)
  for note in pairs(input_tracker.pressed_notes) do
    send{
      off = true,
      note = note,
    }
  end
end)

input_tracker = note_tracker()

local muted = false

hook({ mute = true }, function()
  for note in pairs(tracker.pressed_notes) do
    send{
      off = true,
      note = note,
    }
  end
  muted = true
end)

hook({ unmute = true }, function()
  muted = false
  for note in pairs(tracker.pressed_notes) do
    send{
      on = true,
      note = note,
      vel = latest_note_velocities[note],
    }
  end
end)

-- Let key events through only when not muted.
hook({ key = true }, function(ev)
  if not muted then
    send(ev)
  end
end)
