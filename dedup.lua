function dedup_note_offs(channel_key)
  if channel_key == nil then
    channel_key = 'channel'
  end

  -- For each note, record which channels it is pressed on.
  local pressed_notes = {}

  hook({on = true}, function(ev)
    -- Record note press.
    local k = ev[channel_key]
    if pressed_notes[ev.note] == nil then
      pressed_notes[ev.note] = {}
    end
    pressed_notes[ev.note][k] = true

    -- Send event with no channel information.
    ev[channel_key] = nil
    send(ev)
  end)

  -- Record note releases and filter them so we don't send duplicates.
  hook({off = true}, function(ev)
    -- Record note release.
    local k = ev[channel_key]
    if pressed_notes[ev.note][k] then
      pressed_notes[ev.note][k] = nil
      if next(pressed_notes[ev.note]) == nil then
        -- The note is no longer pressed on any channels!
        pressed_notes[ev.note] = nil

        -- Send event with no channel information.
        ev[channel_key] = nil
        send(ev)
      end
    end
  end)
end

dedup_note_offs()
