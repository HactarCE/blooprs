DEFAULT_OCTAVE = 4
OCTAVE_LEN = 12
BASE_NOTE = 60 -- C4

local NOTE_NAMES = { 'C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B' }
local NOTE_OFFSETS = { C = 0, D = 2, E = 4, F = 5, G = 7, A = 9, B = 11 }
local ACCIDENTALS = { ['#'] = 1, ['b'] = -1 }

function notename(key)
  assert(type(key) == 'number')
  key = key - BASE_NOTE
  local offset = key % OCTAVE_LEN
  local octave = math.floor(key / OCTAVE_LEN) + DEFAULT_OCTAVE
  return NOTE_NAMES[offset+1] .. octave
end

function note(name)
  assert(type(name) == 'string')
  local letter = name:sub(1, 1):upper()
  local note = BASE_NOTE + NOTE_OFFSETS[letter]
  assert(note, string.format('bad note letter: %q', letter))

  local i = name:find('[%d-]', 2) or #name + 1
  if i > 2 then
    local accidental = ACCIDENTALS[name:sub(2, i-1)]
    assert(accidental, string.format('bad accidental: %q', accidental))
    note = note + accidental
  end

  if i <= #name then
    local octave = tonumber(name:sub(i))
    note = note + (octave - DEFAULT_OCTAVE) * OCTAVE_LEN
  end

  return note
end

hook(function(ev)
  ev.note = ev.key
  ev.key = nil
  send(ev)
end)

notename(60) -- 'C4'
note('F#') -- number for F#4
note('F#3') -- number for F#3





function hook_cc()
  local cc_values = {}
  hook({cc = true}, function(ev)
    cc_values[ev.cc] = ev.value
  end)

  setmetatable(cc_values, { __index = 0 })

  return cc_values
end

function save_cc(cc_values)
  local ret = {}
  for k, v in pairs(cc_values) do
    ret[k] = v
  end
  setmetatable(ret, { __index = 0 })
  return ret
end

function restore_cc(cc_values)
  for k, v in pairs(cc_values) do
    send{ cc = k, value = v }
  end
end
