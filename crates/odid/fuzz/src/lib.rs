use odid::{Message, decode_message, parse_pack};

pub fn touch_pack(pack: &[u8]) {
    if let Ok(msgs) = parse_pack(pack) {
        for m in msgs {
            touch_message(m);
        }
    }
}

pub fn touch_message(msg: &[u8; 25]) {
    let Ok(m) = decode_message(msg) else { return };
    match m {
        Message::BasicId(v) => {
            let _ = v.as_str();
        }
        Message::Location(v) => {
            let _ = (
                v.track_deg(),
                v.speed_h_mps(),
                v.speed_v_mps(),
                v.timestamp_s(),
            );
            let _ = (v.latitude_deg(), v.longitude_deg());
            let _ = (v.alt_baro_m(), v.alt_geo_m(), v.height_m());
        }
        Message::Auth(v) => {
            let _ = (v.last_page_index(), v.length(), v.timestamp_unix_s());
        }
        Message::SelfId(v) => {
            let _ = v.as_str();
        }
        Message::System(v) => {
            let _ = (v.latitude_deg(), v.longitude_deg(), v.area_radius_m());
            let _ = (v.area_ceiling_m(), v.area_floor_m(), v.op_alt_geo_m());
            let _ = v.timestamp_unix_s();
        }
        Message::OperatorId(v) => {
            let _ = v.as_str();
        }
    }
}
