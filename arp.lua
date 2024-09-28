MODE = {
  'loop', -- continue looping while chord is held
  'tempoloop', -- continue looping while chord is held
  'queue', -- finish cycle once released; queue next chord
}

MODE = 'queue'

-- bloop 1 -> arpeggio template (tempo sync may be on or off)
initial_note = nil
arpeggio_offsets = {}

hook({ begin_record = true }, function()
  arpeggio_offsets = {}
end)

hook({ live = true }) -- don't care

hook({ record = true, on = true }, function(ev)
  if initial_note == nil then
    initial_note = ev.note
  else
    ev.offset
  end
  send()
end)

hook({ playback = true })

outputs = { bloops = {2} }

-- bloop 2 -> chord control

add_output({ bloop = 2 })
