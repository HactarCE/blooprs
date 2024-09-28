use rlua::prelude::*;

macro_rules! lua_module {
    ($filename:literal) => {
        ($filename, include_str!($filename))
    };
}

const LUA_MODULES: &[(&str, &str)] = &[
    lua_module!("01_pprint.lua"),
    lua_module!("02_logging.lua"),
    lua_module!("03_hooks.lua"),
    lua_module!("04_sandbox.lua"),
];

pub fn new_lua() -> Lua {
    // SAFETY: We need the debug library to get traceback info for better error
    // reporting. We use Lua sandboxing functionality so the user should never
    // be able to access the debug module.
    let lua = unsafe {
        Lua::new_with(
            rlua::StdLib::BASE
                | rlua::StdLib::TABLE
                | rlua::StdLib::STRING
                | rlua::StdLib::UTF8
                | rlua::StdLib::MATH,
        )
    };

    lua.context(|lua| {
        for (module_name, module_source) in LUA_MODULES {
            log::info!("Loading Lua module {module_name:?}");
            if let Err(e) = lua.load(module_source).set_name(module_name)?.exec() {
                panic!("error loading Lua module {module_name:?}:\n\n{e}\n\n");
            }
        }

        // Grab the sandbox environment so we can insert our custom globals.
        let sandbox: LuaTable<'_> = lua.globals().get("SANDBOX_ENV")?;

        // Constants
        let blooprs_version_string =
            format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        sandbox.raw_set("_BLOOPRS", blooprs_version_string)?;

        LuaResult::Ok(())
    })
    .expect("error initializing lua");

    lua
}
