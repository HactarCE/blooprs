local last_hook_id = 0
ADDED_HOOKS = {}
function hook(...)
  local filter, callback
  if select('#', ...) == 2 then
    filter, callback = ...
  elseif select('#', ...) == 1 then
    callback = ...
  else
    error("expected 1 or 2 arguments to 'hook([filter], callback)'")
  end

  assert(
    type(filter) == 'table',
    "expected table or nil for 'filter' in 'hook([filter], callback)'"
  )

  assert(
    type(callback) == 'function',
    "expected function for 'callback' in 'hook([filter], callback)'"
  )

  last_hook_id = last_hook_id + 1
  table.insert(ADDED_HOOKS, {
    id = last_hook_id,
    filter = filter,
    callback = callback,
  })
  return last_hook_id
end

REMOVED_HOOKS = {}
function unhook(id)
  assert(type(id) == 'number', "expected number for 'callback' in 'unhook(callback)'")
  table.insert(REMOVED_HOOKS, id)
end

EVENTS_TO_SEND = {}
function send(event)
  assert(type(event) == 'table', "expected table for 'event' in 'send(event)'")
  table.insert(EVENTS_TO_SEND, event)
end

CLEAR_QUEUE = {}
function cancel(filter)
  if filter == nil then filter = {} end
  assert(type(filter) ~= 'table', "expected table or nil for 'filter' in 'cancel([filter])'")
  table.insert(CLEAR_QUEUE, filter or {})
end
