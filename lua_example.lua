-- Lua Example

math.round = function(x, subdiv)
  if subdiv == nil then subdiv = 1 end
  if type(x) ~= 'number' or type(subdiv) ~= 'number' then
    error('math.round(x, [subdiv]) requires numbers')
  end
  return math.floor(x * subdiv + 0.5) / subdiv
end

event = {
  time = {
    measure = N, -- integer
    beat = N, -- fractional
  },

  -- cancel = true/nil, -- whether this is a cancel request???

  cctrack = { 64 }, -- track pedal input (so you can use `cc(64)` in the hook)

  live = true/false,
  record = true/false,
  playback = true/false,

  on = true/nil,
  off = true/nil,
  aftertouch = true/nil,
  key = N, -- integer (0 to 127)
  vel = N, -- integer (0 to 127)

  cc = N, -- integer (0 to 127)
  value = N, -- integer (0 to 127)

  program = N, -- integer (0 to 127)

  bend = N, -- integer (-0x2000 to +0x3FFF)
}


event = {
  time,

}

retime_hook(function(ev)
  ev.beat = round(ev.beat)
  send(ev)
end)


quantize(4) -- sixteenth notes

remap('f#5', 'f#4')
end_remaps()






function remap(key_in, key_out)
  return hook(
    { key = key_in, remapped = false },
    function(ev)
      ev.key = key_out
      ev.remapped = true
      send(ev)
    end
  )
end
function end_remaps()
  hook({ remapped = false }, function() end) -- ignore unmapped
  hook(function(ev) ev.remapped = nil end) -- remove `remapped` flag
end

function quantize(beat_subdivisions)
  return hook(function(ev)
    ev.beat = round(ev.beat, beat_subdivisions)
    return ev
  end)
end

function remap_next(key_in, key_out)
  return hook(
    { type = 'on', key = key_in },
    function(ev)
      ev.key = key_out
      send(ev)
      unhook()
      hook(
        { key = key_in },
        function(ev)
          ev.key = key_out
          send(ev)
          if ev.off then
            unhook()
          end
        end
      )
    end
  )
end

function hookonce(params, callback)
  return hook(params, function(ev)
    callback(ev)
    unhook()
  end)
end
