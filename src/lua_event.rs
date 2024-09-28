use midly::num::u7;

// pub struct LuaEvent {
//     time: LuaTime,
//     data: LuaEventData,
// }

// pub struct LuaTime {
//     measure: Option<u64>,
//     beat: Option<u32>,
// }

// pub enum LuaEventData {
//     Cancel {
//         cancel: bool,

//         key: Option<i64>,
//         cc: Option<i64>,
//     },
//     Key {
//         on: Option<bool>,
//         off: Option<bool>,
//         aftertouch: Option<bool>,

//         key: i64,
//         vel: Option<u7>,
//     },
//     Controller {
//         cc: u64,
//         value: u7,
//     },
//     ProgramChange {
//         program: u7,
//     },
//     PitchBend {
//         bend: i16,
//     },
// }
