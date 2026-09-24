use crate::MESSAGE_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Message {
    BasicId(BasicId),
    Location(Location),
    Auth(AuthPage),
    SelfId(SelfId),
    System(System),
    OperatorId(OperatorId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    NestedPack,
    UnknownType(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BasicId {
    pub proto_version: u8,
    pub id_type: u8,
    pub ua_type: u8,
    pub uas_id: [u8; 20],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    pub proto_version: u8,
    pub status: u8,
    pub height_type: u8,
    pub ew_direction: u8,
    pub speed_mult: u8,
    pub direction_raw: u8,
    pub speed_h_raw: u8,
    pub speed_v_raw: i8,
    pub lat_e7: i32,
    pub lon_e7: i32,
    pub alt_baro_raw: u16,
    pub alt_geo_raw: u16,
    pub height_raw: u16,
    pub vert_acc: u8,
    pub horiz_acc: u8,
    pub baro_acc: u8,
    pub speed_acc: u8,
    pub timestamp_raw: u16,
    pub ts_acc: u8,
}

/// Bytes 2–24 stay raw in `data`; page-0 fields are read through accessors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthPage {
    pub proto_version: u8,
    pub auth_type: u8,
    pub data_page: u8,
    pub data: [u8; 23],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelfId {
    pub proto_version: u8,
    pub desc_type: u8,
    pub desc: [u8; 23],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct System {
    pub proto_version: u8,
    pub classification_type: u8,
    pub operator_location_type: u8,
    pub op_lat_e7: i32,
    pub op_lon_e7: i32,
    pub area_count: u16,
    pub area_radius_raw: u8,
    pub area_ceiling_raw: u16,
    pub area_floor_raw: u16,
    pub category_eu: u8,
    pub class_eu: u8,
    pub op_alt_geo_raw: u16,
    pub timestamp: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperatorId {
    pub proto_version: u8,
    pub operator_id_type: u8,
    pub operator_id: [u8; 20],
}

fn u16_at(m: &[u8; MESSAGE_SIZE], i: usize) -> u16 {
    let mut b = [0; 2];
    if let Some(s) = m.get(i..).and_then(|s| s.first_chunk::<2>()) {
        b = *s;
    }
    u16::from_le_bytes(b)
}

fn u32_at(m: &[u8; MESSAGE_SIZE], i: usize) -> u32 {
    let mut b = [0; 4];
    if let Some(s) = m.get(i..).and_then(|s| s.first_chunk::<4>()) {
        b = *s;
    }
    u32::from_le_bytes(b)
}

pub fn decode_message(msg: &[u8; MESSAGE_SIZE]) -> Result<Message, DecodeError> {
    let m = msg;
    let proto_version = m[0] & 0x0F;
    let (hi1, lo1) = (m[1] >> 4, m[1] & 0x0F);
    Ok(match m[0] >> 4 {
        0 => {
            let [_, _, uas_id @ .., _, _, _] = *m;
            Message::BasicId(BasicId {
                proto_version,
                id_type: hi1,
                ua_type: lo1,
                uas_id,
            })
        }
        1 => Message::Location(Location {
            proto_version,
            status: hi1,
            height_type: (m[1] >> 2) & 1,
            ew_direction: (m[1] >> 1) & 1,
            speed_mult: m[1] & 1,
            direction_raw: m[2],
            speed_h_raw: m[3],
            speed_v_raw: m[4].cast_signed(),
            lat_e7: u32_at(m, 5).cast_signed(),
            lon_e7: u32_at(m, 9).cast_signed(),
            alt_baro_raw: u16_at(m, 13),
            alt_geo_raw: u16_at(m, 15),
            height_raw: u16_at(m, 17),
            vert_acc: m[19] >> 4,
            horiz_acc: m[19] & 0x0F,
            baro_acc: m[20] >> 4,
            speed_acc: m[20] & 0x0F,
            timestamp_raw: u16_at(m, 21),
            ts_acc: m[23] & 0x0F,
        }),
        2 => {
            let [_, _, data @ ..] = *m;
            Message::Auth(AuthPage {
                proto_version,
                auth_type: hi1,
                data_page: lo1,
                data,
            })
        }
        3 => {
            let [_, desc_type, desc @ ..] = *m;
            Message::SelfId(SelfId {
                proto_version,
                desc_type,
                desc,
            })
        }
        4 => Message::System(System {
            proto_version,
            classification_type: (m[1] >> 2) & 0x07,
            operator_location_type: m[1] & 0x03,
            op_lat_e7: u32_at(m, 2).cast_signed(),
            op_lon_e7: u32_at(m, 6).cast_signed(),
            area_count: u16_at(m, 10),
            area_radius_raw: m[12],
            area_ceiling_raw: u16_at(m, 13),
            area_floor_raw: u16_at(m, 15),
            category_eu: m[17] >> 4,
            class_eu: m[17] & 0x0F,
            op_alt_geo_raw: u16_at(m, 18),
            timestamp: u32_at(m, 20),
        }),
        5 => {
            let [_, operator_id_type, operator_id @ .., _, _, _] = *m;
            Message::OperatorId(OperatorId {
                proto_version,
                operator_id_type,
                operator_id,
            })
        }
        0xF => return Err(DecodeError::NestedPack),
        t => return Err(DecodeError::UnknownType(t)),
    })
}
