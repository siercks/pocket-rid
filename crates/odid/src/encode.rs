use crate::{
    ASTM_OUI, AuthPage, BasicId, Location, MAX_PACK_MESSAGES, MESSAGE_SIZE, ODID_OUI_TYPE,
    OperatorId, SelfId, System,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    BufferTooSmall,
    TooManyMessages,
}

type Msg = [u8; MESSAGE_SIZE];

fn header(ty: u8, version: u8) -> u8 {
    (ty << 4) | (version & 0x0F)
}

fn put(m: &mut Msg, at: usize, b: &[u8]) {
    if let Some(d) = m.get_mut(at..).and_then(|d| d.get_mut(..b.len())) {
        d.copy_from_slice(b);
    }
}

pub fn encode_basic_id(v: &BasicId) -> Msg {
    let mut m = [0; MESSAGE_SIZE];
    m[0] = header(0, v.proto_version);
    m[1] = (v.id_type << 4) | (v.ua_type & 0x0F);
    put(&mut m, 2, &v.uas_id);
    m
}

pub fn encode_location(v: &Location) -> Msg {
    let mut m = [0; MESSAGE_SIZE];
    m[0] = header(1, v.proto_version);
    m[1] = (v.status << 4)
        | ((v.height_type & 1) << 2)
        | ((v.ew_direction & 1) << 1)
        | (v.speed_mult & 1);
    m[2] = v.direction_raw;
    m[3] = v.speed_h_raw;
    m[4] = v.speed_v_raw.cast_unsigned();
    put(&mut m, 5, &v.lat_e7.to_le_bytes());
    put(&mut m, 9, &v.lon_e7.to_le_bytes());
    put(&mut m, 13, &v.alt_baro_raw.to_le_bytes());
    put(&mut m, 15, &v.alt_geo_raw.to_le_bytes());
    put(&mut m, 17, &v.height_raw.to_le_bytes());
    m[19] = (v.vert_acc << 4) | (v.horiz_acc & 0x0F);
    m[20] = (v.baro_acc << 4) | (v.speed_acc & 0x0F);
    put(&mut m, 21, &v.timestamp_raw.to_le_bytes());
    m[23] = v.ts_acc & 0x0F;
    m
}

pub fn encode_auth(v: &AuthPage) -> Msg {
    let mut m = [0; MESSAGE_SIZE];
    m[0] = header(2, v.proto_version);
    m[1] = (v.auth_type << 4) | (v.data_page & 0x0F);
    put(&mut m, 2, &v.data);
    m
}

pub fn encode_self_id(v: &SelfId) -> Msg {
    let mut m = [0; MESSAGE_SIZE];
    m[0] = header(3, v.proto_version);
    m[1] = v.desc_type;
    put(&mut m, 2, &v.desc);
    m
}

pub fn encode_system(v: &System) -> Msg {
    let mut m = [0; MESSAGE_SIZE];
    m[0] = header(4, v.proto_version);
    m[1] = ((v.classification_type & 0x07) << 2) | (v.operator_location_type & 0x03);
    put(&mut m, 2, &v.op_lat_e7.to_le_bytes());
    put(&mut m, 6, &v.op_lon_e7.to_le_bytes());
    put(&mut m, 10, &v.area_count.to_le_bytes());
    m[12] = v.area_radius_raw;
    put(&mut m, 13, &v.area_ceiling_raw.to_le_bytes());
    put(&mut m, 15, &v.area_floor_raw.to_le_bytes());
    m[17] = (v.category_eu << 4) | (v.class_eu & 0x0F);
    put(&mut m, 18, &v.op_alt_geo_raw.to_le_bytes());
    put(&mut m, 20, &v.timestamp.to_le_bytes());
    m
}

pub fn encode_operator_id(v: &OperatorId) -> Msg {
    let mut m = [0; MESSAGE_SIZE];
    m[0] = header(5, v.proto_version);
    m[1] = v.operator_id_type;
    put(&mut m, 2, &v.operator_id);
    m
}

/// Writes a protocol-version-2 pack header followed by `msgs`.
pub fn build_pack(msgs: &[Msg], out: &mut [u8]) -> Result<usize, EncodeError> {
    if msgs.len() > MAX_PACK_MESSAGES {
        return Err(EncodeError::TooManyMessages);
    }
    let body = msgs.as_flattened();
    let (head, rest) = out
        .split_first_chunk_mut::<3>()
        .ok_or(EncodeError::BufferTooSmall)?;
    *head = [0xF2, MESSAGE_SIZE as u8, msgs.len() as u8];
    rest.get_mut(..body.len())
        .ok_or(EncodeError::BufferTooSmall)?
        .copy_from_slice(body);
    Ok(body.len().saturating_add(3))
}

/// Minimal 5.1 beacon: fixed header with SA = BSSID = `mac`, then only the Remote ID element.
pub fn build_beacon(
    mac: [u8; 6],
    counter: u8,
    pack: &[u8],
    out: &mut [u8],
) -> Result<usize, EncodeError> {
    let ie_len =
        u8::try_from(pack.len().saturating_add(5)).map_err(|_| EncodeError::BufferTooSmall)?;
    let (head, rest) = out
        .split_first_chunk_mut::<43>()
        .ok_or(EncodeError::BufferTooSmall)?;
    *head = [0; 43];
    head[0] = 0x80;
    head[4..10].copy_from_slice(&[0xFF; 6]);
    head[10..16].copy_from_slice(&mac);
    head[16..22].copy_from_slice(&mac);
    head[36] = 0xDD;
    head[37] = ie_len;
    head[38..41].copy_from_slice(&ASTM_OUI);
    head[41] = ODID_OUI_TYPE;
    head[42] = counter;
    rest.get_mut(..pack.len())
        .ok_or(EncodeError::BufferTooSmall)?
        .copy_from_slice(pack);
    Ok(pack.len().saturating_add(43))
}
