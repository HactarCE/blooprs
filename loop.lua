function quantize(ev)
  -- shallowcopy `ev`
  local ret = {}
  for k, v in pairs(ev) do
    ret[k] = v
  end
  -- TODO: implement quantization
  return ret
end


-- Round recording times to next measure.
local function at_next_measure(ev)
  if ev.time.measure then
    ev.time = Time{ measure = ev.time.measure + 1 }
  end
  return ev
end
hook({ start_recording = true }, function()
  time.mark_power_of_2()
  send(at_next_measure{ start_recording = true })
end)
hook({ stop_recording = true }, function()
  time.mark_power_of_2()
  send(at_next_measure{ stop_recording = true })
end)

effect('Looper', function()

  -- Record the latest velocity of each key.
  local latest_note_velocities = velocity_tracker()

  -- Record the latest value of each CC.
  local latest_cc_values = cc_tracker()

  local recording = false
  local recording_start_time
  local recording_duration
  local input_tracker = note_tracker()
  local recording_tracker = note_tracker()
  local initial_note_velocities = {}
  local initial_cc_values = {}

  local recording_buffer = {}
  local measure_count = 0

  hook({ start_recording = true }, function(ev)
    recording = true
    recording_start_time = ev.time

    -- Record initial states.
    recording_tracker.enabled = true
    recording_tracker:copy_state_from(input_tracker)

    -- Record initial velocities.
    initial_note_velocities = {}
    for k, v in pairs(latest_note_velocities) do
      if recording_tracker.pressed_notes[k] then
        initial_note_velocities[k] = v
      end
    end

    -- Record initial CC values.
    for k, v in pairs(latest_cc_values) do
      initial_cc_values[k] = v
    end
  end)

  -- Record note-on and non-note-related events iff we are currently recording.
  hook({ note = false }, function(ev)
    if recording then
      table.insert(recording_buffer, table.shallowcopy(ev))
    end
    send(quantize(ev))
  end)
  hook({ note = true, on = true }, function(ev)
    if recording then
      table.insert(recording_buffer, table.shallowcopy(ev))
    end
    send(quantize(ev))
  end)
  -- For aftertouch and note-off events, record iff the key has been held since a
  -- time when we were recording.
  hook({ note = true, on = false }, function(ev)
    if recording_tracker.pressed_notes[ev.note] then
      table.insert(recording_buffer, table.shallowcopy(ev))
    end
    send(quantize(ev))
  end)

  hook({ stop_recording = true }, function(ev)
    recording = false
    recording_duration = ev.time - recording_start_time
    send{ queue_playback = true }
  end)

  local output_tracker

  hook({ queue_playback = true }, function(ev)
    -- Ensure CCs have the correct values.
    for cc, value in ipairs(latest_cc_values) do
      send{ cc = cc, value = initial_cc_values[cc] }
    end

    -- Ensure notes are pressed.
    for note, vel in ipairs(initial_note_velocities) do
      if not output_tracker.pressed_notes[note] then
        send{ on = true, note = note, vel = vel }
      end
    end

    for _, e in ipairs(recording_buffer) do
      e.time = e.time + recording_duration
      send(quantize(e))
    end
    send{
      time = ev.time + recording_duration,
      queue_playback = true,
      _send_to_self = true,
    }
  end)

  output_tracker = note_tracker()

  hook({ clear_recording = true }, function()
    recording_buffer = {}
    clear_queue()
    send{ cancel = true } -- Cancel notes currently being played.
    -- TODO: do not cancel notes currently being held by the user
  end)

  local output_tracker = note_tracker()


  display(function(ui)
    if recording then
      ui:label("Recording")
    else
      ui:label("Not recording")
    end
  end)

end)
