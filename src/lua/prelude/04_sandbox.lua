-- See http://lua-users.org/wiki/SandBoxes

SANDBOX_ENV = {
  -- Built-in constants
  _VERSION = _VERSION,

  -- Safe built-in functions
  assert = assert,
  error = error,
  ipairs = ipairs,
  next = next,
  pairs = pairs,
  pcall = pcall,
  print = print,
  select = select,
  tonumber = tonumber,
  tostring = tostring,
  type = type,
  unpack = unpack,
  warn = warn,

  -- Questionable built-in functions
  getmetatable = getmetatable,
  rawequal = rawequal,
  rawget = rawget,
  rawset = rawset,
  setmetatable = setmetatable,

  -- Safe built-in modules
  math = math,
  string = string,
  table = table,
  utf8 = utf8,

  -- Safe custom functions
  pstring = pstring,
  pprint = pprint,

  -- Library access
  hook = hook,
  unhook = unhook,

  -- Rust code will inject more entries into this table
}

-- Prevent modifications to globals
READ_ONLY_METATABLE = {
  __newindex = function() error('cannot overwrite bulitins') end,
  __metatable = 'nice try',
}
setmetatable(math, READ_ONLY_METATABLE)
setmetatable(string, READ_ONLY_METATABLE)
setmetatable(table, READ_ONLY_METATABLE)
setmetatable(utf8, READ_ONLY_METATABLE)
setmetatable(SANDBOX_ENV, READ_ONLY_METATABLE)

function make_sandbox()
  -- Construct a new table so that it's easy to see what globals have been added
  -- by the user
  local sandbox = {}
  sandbox._G = sandbox

  -- `__index` is ok because modules are protected via metatable
  -- (and we do not give users the ability to manipulate/bypass metatable)
  setmetatable(sandbox, {__index = SANDBOX_ENV})

  return sandbox
end
