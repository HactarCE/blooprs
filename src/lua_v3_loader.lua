-- Protect the metatables of strings
getmetatable("").__metatable = 'nice try'

function table.shallowcopy(t)
  local ret = {}
  for k, v in pairs(t) do
    ret[k] = v
  end
  return ret
end

local LOADED_FILES = {}

SANDBOX_ENV = {
  -- Built-in constants
  _VERSION = _VERSION,

  -- Safe built-in functions
  ipairs = ipairs,
  next = next,
  pcall = pcall,
  pairs = pairs,
  select = select,
  tonumber = tonumber,
  tostring = tostring,
  type = type,
  unpack = unpack,

  -- Questionable built-in functions
  getmetatable = getmetatable,
  rawequal = rawequal,
  rawget = rawget,
  rawset = rawset,
  setmetatable = setmetatable,

  -- Safe built-in modules
  math = table.shallowcopy(math),
  string = table.shallowcopy(string),
  table = table.shallowcopy(table),
  utf8 = table.shallowcopy(utf8),

  -- Safe custom functions
  assert = assert,
  error = error,
  warn = function(...) warn(FILE.name, ...) end,
  info = function(...) info(FILE.name, ...) end,
  pstring = pstring,
  print = function(...) info(FILE.name, ...) end,
  pprint = function(...) info(FILE.name, pstring(...)) end,

  -- Safe utility functions
  collect = collect,
  iter = iter,

  -- Library access
  puzzledef = puzzledef,
  require = require,

  -- Rust code will inject more entries into this table
}

function load(filename, contents)
  SANDBOX =
  load(contents)
  LOADED_FILES
end
