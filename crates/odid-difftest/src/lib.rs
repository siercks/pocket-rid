//! FFI to the reference opendroneid-core-c decoder. Test-only; never linked into firmware.
//! This crate is the one approved exception to the `unsafe` rule (DECISIONS.md, M10).

#[repr(C)]
#[derive(Default)]
pub struct BasicIdData {
    pub ua_type: u32,
    pub id_type: u32,
    pub uas_id: [u8; 21],
}

#[repr(C)]
#[derive(Default)]
pub struct LocationData {
    pub status: u32,
    pub direction: f32,
    pub speed_horizontal: f32,
    pub speed_vertical: f32,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_baro: f32,
    pub altitude_geo: f32,
    pub height_type: u32,
    pub height: f32,
    pub horiz_accuracy: u32,
    pub vert_accuracy: u32,
    pub baro_accuracy: u32,
    pub speed_accuracy: u32,
    pub ts_accuracy: u32,
    pub timestamp: f32,
}

#[repr(C)]
#[derive(Default)]
pub struct AuthData {
    pub data_page: u8,
    pub auth_type: u32,
    pub last_page_index: u8,
    pub length: u8,
    pub timestamp: u32,
    pub auth_data: [u8; 24],
}

#[repr(C)]
#[derive(Default)]
pub struct SelfIdData {
    pub desc_type: u32,
    pub desc: [u8; 24],
}

#[repr(C)]
#[derive(Default)]
pub struct SystemData {
    pub operator_location_type: u32,
    pub classification_type: u32,
    pub operator_latitude: f64,
    pub operator_longitude: f64,
    pub area_count: u16,
    pub area_radius: u16,
    pub area_ceiling: f32,
    pub area_floor: f32,
    pub category_eu: u32,
    pub class_eu: u32,
    pub operator_altitude_geo: f32,
    pub timestamp: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct OperatorIdData {
    pub operator_id_type: u32,
    pub operator_id: [u8; 21],
}

unsafe extern "C" {
    fn decodeBasicIDMessage(out: *mut BasicIdData, msg: *const u8) -> i32;
    fn decodeLocationMessage(out: *mut LocationData, msg: *const u8) -> i32;
    fn decodeAuthMessage(out: *mut AuthData, msg: *const u8) -> i32;
    fn decodeSelfIDMessage(out: *mut SelfIdData, msg: *const u8) -> i32;
    fn decodeSystemMessage(out: *mut SystemData, msg: *const u8) -> i32;
    fn decodeOperatorIDMessage(out: *mut OperatorIdData, msg: *const u8) -> i32;
    fn decodeMessagePack(uas: *mut u8, pack: *const u8) -> i32;
}

macro_rules! wrap {
    ($name:ident, $c:ident, $t:ty) => {
        pub fn $name(msg: &[u8; 25]) -> Option<$t> {
            let mut out = <$t>::default();
            // SAFETY: `msg` is 25 readable bytes, the size of every encoded message; `out` is a
            // valid, correctly laid out `repr(C)` mirror of the C output struct.
            (unsafe { $c(&mut out, msg.as_ptr()) } == 0).then_some(out)
        }
    };
}

wrap!(basic_id, decodeBasicIDMessage, BasicIdData);
wrap!(location, decodeLocationMessage, LocationData);
wrap!(auth, decodeAuthMessage, AuthData);
wrap!(self_id, decodeSelfIDMessage, SelfIdData);
wrap!(system, decodeSystemMessage, SystemData);
wrap!(operator_id, decodeOperatorIDMessage, OperatorIdData);

/// `true` if the reference decoder accepts the pack. `pack` must hold 3 + 25 × 9 bytes.
pub fn pack_ok(pack: &[u8; 228]) -> bool {
    // Larger than ODID_UAS_Data (about 1 KiB) and 8-byte aligned; its contents are not inspected.
    let mut uas = [0u64; 512];
    // SAFETY: `pack` covers the largest pack the decoder reads (MsgPackSize is range-checked to
    // ≤ 9 before any message is touched); `uas` is a zeroed buffer larger than ODID_UAS_Data.
    unsafe { decodeMessagePack(uas.as_mut_ptr().cast(), pack.as_ptr()) == 0 }
}
