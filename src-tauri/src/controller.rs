use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerSnapshot {
    pub connected: bool,
    pub left_stick_x: f32,
    pub left_stick_y: f32,
    pub right_stick_x: f32,
    pub right_stick_y: f32,
    pub left_trigger: f32,
    pub right_trigger: f32,
    pub battery_percent: Option<u8>,
    pub buttons: HashMap<String, bool>,
}

impl ControllerSnapshot {
    fn disconnected() -> Self {
        Self {
            connected: false,
            left_stick_x: 0.0,
            left_stick_y: 0.0,
            right_stick_x: 0.0,
            right_stick_y: 0.0,
            left_trigger: 0.0,
            right_trigger: 0.0,
            battery_percent: None,
            buttons: HashMap::new(),
        }
    }
}

#[cfg(windows)]
#[repr(C)]
struct XInputGamepad {
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    thumb_lx: i16,
    thumb_ly: i16,
    thumb_rx: i16,
    thumb_ry: i16,
}

#[cfg(windows)]
#[repr(C)]
struct XInputState {
    packet_number: u32,
    gamepad: XInputGamepad,
}

#[cfg(windows)]
type XInputGetState = unsafe extern "system" fn(u32, *mut XInputState) -> u32;

#[cfg(windows)]
#[repr(C)]
struct XInputBatteryInformation {
    battery_type: u8,
    battery_level: u8,
}

#[cfg(windows)]
type XInputGetBatteryInformation =
    unsafe extern "system" fn(u32, u8, *mut XInputBatteryInformation) -> u32;

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut std::ffi::c_void;
    fn GetProcAddress(module: *mut std::ffi::c_void, name: *const u8) -> *mut std::ffi::c_void;
}

#[cfg(windows)]
fn xinput_get_state() -> Option<XInputGetState> {
    static XINPUT_ADDRESS: OnceLock<usize> = OnceLock::new();
    let address = *XINPUT_ADDRESS.get_or_init(|| {
        for dll in ["xinput1_4.dll", "xinput9_1_0.dll"] {
            let wide: Vec<u16> = dll.encode_utf16().chain(std::iter::once(0)).collect();
            let module = unsafe { LoadLibraryW(wide.as_ptr()) };
            if !module.is_null() {
                let proc = unsafe { GetProcAddress(module, c"XInputGetState".as_ptr().cast()) };
                if !proc.is_null() {
                    return proc as usize;
                }
            }
        }
        0
    });
    if address == 0 {
        None
    } else {
        Some(unsafe { std::mem::transmute::<usize, XInputGetState>(address) })
    }
}

#[cfg(windows)]
fn xinput_get_battery_information() -> Option<XInputGetBatteryInformation> {
    static XINPUT_BATTERY_ADDRESS: OnceLock<usize> = OnceLock::new();
    let address = *XINPUT_BATTERY_ADDRESS.get_or_init(|| {
        for dll in ["xinput1_4.dll", "xinput9_1_0.dll"] {
            let wide: Vec<u16> = dll.encode_utf16().chain(std::iter::once(0)).collect();
            let module = unsafe { LoadLibraryW(wide.as_ptr()) };
            if !module.is_null() {
                let proc = unsafe {
                    GetProcAddress(module, c"XInputGetBatteryInformation".as_ptr().cast())
                };
                if !proc.is_null() {
                    return proc as usize;
                }
            }
        }
        0
    });
    if address == 0 {
        None
    } else {
        Some(unsafe { std::mem::transmute::<usize, XInputGetBatteryInformation>(address) })
    }
}

#[cfg(windows)]
pub fn read_xinput() -> ControllerSnapshot {
    const A: u16 = 0x1000;
    const B: u16 = 0x2000;
    const X: u16 = 0x4000;
    const Y: u16 = 0x8000;
    const DU: u16 = 0x0001;
    const DD: u16 = 0x0002;
    const DL: u16 = 0x0004;
    const DR: u16 = 0x0008;
    const START: u16 = 0x0010;
    const BACK: u16 = 0x0020;
    const L3: u16 = 0x0040;
    const R3: u16 = 0x0080;
    const LB: u16 = 0x0100;
    const RB: u16 = 0x0200;

    let Some(get_state) = xinput_get_state() else {
        return ControllerSnapshot::disconnected();
    };
    let mut raw = XInputState {
        packet_number: 0,
        gamepad: XInputGamepad {
            buttons: 0,
            left_trigger: 0,
            right_trigger: 0,
            thumb_lx: 0,
            thumb_ly: 0,
            thumb_rx: 0,
            thumb_ry: 0,
        },
    };
    let Some(user_index) = (0..4).find(|&i| unsafe { get_state(i, &mut raw) } == 0) else {
        return ControllerSnapshot::disconnected();
    };
    let button_bits = raw.gamepad.buttons;
    let mut buttons = HashMap::new();
    for (name, mask) in [
        ("a", A),
        ("b", B),
        ("x", X),
        ("y", Y),
        ("dpad-up", DU),
        ("dpad-down", DD),
        ("dpad-left", DL),
        ("dpad-right", DR),
        ("lb", LB),
        ("rb", RB),
        ("l3", L3),
        ("r3", R3),
        ("menu", START),
        ("view", BACK),
    ] {
        buttons.insert(name.to_string(), button_bits & mask != 0);
    }
    let battery_percent = xinput_get_battery_information().and_then(|get_battery| {
        let mut battery = XInputBatteryInformation {
            battery_type: 0,
            battery_level: 0,
        };
        if unsafe { get_battery(user_index, 0, &mut battery) } != 0
            || battery.battery_type == 0
            || battery.battery_type == 1
        {
            return None;
        }
        Some(match battery.battery_level {
            0 => 10,
            1 => 35,
            2 => 65,
            _ => 100,
        })
    });
    ControllerSnapshot {
        connected: true,
        left_stick_x: raw.gamepad.thumb_lx as f32 / 32767.0,
        left_stick_y: raw.gamepad.thumb_ly as f32 / 32767.0,
        right_stick_x: raw.gamepad.thumb_rx as f32 / 32767.0,
        right_stick_y: raw.gamepad.thumb_ry as f32 / 32767.0,
        left_trigger: raw.gamepad.left_trigger as f32 / 255.0,
        right_trigger: raw.gamepad.right_trigger as f32 / 255.0,
        battery_percent,
        buttons,
    }
}
