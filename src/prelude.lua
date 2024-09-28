_GLOBAL_PRE_HOOKS = {

}

_BLOOP_HOOKS = {

}

_GLOBAL_POST_HOOKS = {

}

_HOOK_ZONE = nil

time = {}

local epoch
local 

function time.mark_power_of_2()

end

function hook(...)
  local filter, callback
  if select('#', ...) == 1 then
    callback = ...
  elseif select('#', ...) == 2 then
    filter, callback = ...
  else
    error('expected one or two arguments passed to `hook([filter], callback)`')
  end
  if filter ~= nil and type(filter) ~= 'table' then
    error('`filter` in `hook([filter], callback)` must be nil or table')
  end
  if type(callback) ~= 'function' then
    error('`callback` in `hook([filter], callback)` must be function')
  end
  table.insert(_HOOK_ZONE, {filter, callback})
end
