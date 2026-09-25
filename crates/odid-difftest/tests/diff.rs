use odid::{Message, decode_message, parse_pack};
use odid_difftest as c;

const N: u32 = 1_000_000;

struct XorShift32(u32);

impl XorShift32 {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    fn byte(&mut self) -> u8 {
        self.next() as u8
    }
}

/// C copies text with `strncpy`: bytes after the first NUL become NUL.
fn nul_fill<const L: usize>(b: &[u8; L]) -> [u8; L] {
    let mut out = [0; L];
    let n = b.iter().position(|&x| x == 0).unwrap_or(L);
    out[..n].copy_from_slice(&b[..n]);
    out
}

fn alt(raw: u16) -> f32 {
    raw as f32 * 0.5 - 1000.0
}

/// Raw integer the C decoder saw, recovered from its `raw / 1e7` output.
fn e7(v: f64) -> i32 {
    (v * 1e7).round() as i32
}

fn check_opt(ours: Option<f32>, c: f32, invalid: f32) -> bool {
    match ours {
        Some(v) => v == c,
        None => c == invalid,
    }
}

fn check_pos(ours: Option<f64>, c: f64) -> bool {
    ours.is_none_or(|v| (v - c).abs() < 1e-12)
}

/// Returns `None` when the reference decoder rejects the message, else whether all fields agree.
fn compare(m: &[u8; 25]) -> Option<bool> {
    let ours = decode_message(m).expect("types 0-5 always decode");
    Some(match ours {
        Message::BasicId(v) => {
            let r = c::basic_id(m)?;
            let text = if matches!(v.id_type, 1 | 2) {
                nul_fill(&v.uas_id)
            } else {
                v.uas_id
            };
            r.ua_type == v.ua_type.into() && r.id_type == v.id_type.into() && r.uas_id[..20] == text
        }
        Message::Location(v) => {
            let r = c::location(m)?;
            let dir = v.direction_raw as f32 + if v.ew_direction != 0 { 180.0 } else { 0.0 };
            let speed_h = match v.speed_mult {
                0 => v.speed_h_raw as f32 * 0.25,
                _ => v.speed_h_raw as f32 * 0.75 + 63.75,
            };
            let ts_ok = match v.timestamp_raw {
                0xFFFF => r.timestamp == 65535.0,
                t => (r.timestamp * 10.0).round() as u16 == t,
            };
            r.status == v.status.into()
                && r.direction == dir
                && v.track_deg().is_none_or(|t| t as f32 == r.direction)
                && r.speed_horizontal == speed_h
                && check_opt(v.speed_h_mps(), r.speed_horizontal, 255.0)
                && r.speed_vertical == v.speed_v_raw as f32 * 0.5
                && check_opt(v.speed_v_mps(), r.speed_vertical, 63.0)
                && e7(r.latitude) == v.lat_e7
                && e7(r.longitude) == v.lon_e7
                && check_pos(v.latitude_deg(), r.latitude)
                && check_pos(v.longitude_deg(), r.longitude)
                && r.altitude_baro == alt(v.alt_baro_raw)
                && r.altitude_geo == alt(v.alt_geo_raw)
                && r.height == alt(v.height_raw)
                && check_opt(v.alt_baro_m(), r.altitude_baro, -1000.0)
                && check_opt(v.alt_geo_m(), r.altitude_geo, -1000.0)
                && check_opt(v.height_m(), r.height, -1000.0)
                && r.height_type == v.height_type.into()
                && r.horiz_accuracy == v.horiz_acc.into()
                && r.vert_accuracy == v.vert_acc.into()
                && r.baro_accuracy == v.baro_acc.into()
                && r.speed_accuracy == v.speed_acc.into()
                && r.ts_accuracy == v.ts_acc.into()
                && ts_ok
        }
        Message::Auth(v) => {
            let r = c::auth(m)?;
            let head = r.auth_type == v.auth_type.into() && r.data_page == v.data_page;
            head && if v.data_page == 0 {
                Some(r.last_page_index) == v.last_page_index()
                    && Some(r.length) == v.length()
                    && Some(u64::from(r.timestamp) + odid::ODID_EPOCH_UNIX_S)
                        == v.timestamp_unix_s()
                    && r.auth_data[..17] == v.data[6..]
            } else {
                r.auth_data[..23] == v.data
            }
        }
        Message::SelfId(v) => {
            let r = c::self_id(m)?;
            r.desc_type == v.desc_type.into() && r.desc[..23] == nul_fill(&v.desc)
        }
        Message::System(v) => {
            let r = c::system(m)?;
            r.operator_location_type == v.operator_location_type.into()
                && r.classification_type == v.classification_type.into()
                && e7(r.operator_latitude) == v.op_lat_e7
                && e7(r.operator_longitude) == v.op_lon_e7
                && check_pos(v.latitude_deg(), r.operator_latitude)
                && check_pos(v.longitude_deg(), r.operator_longitude)
                && r.area_count == v.area_count
                && r.area_radius == u16::from(v.area_radius_raw) * 10
                && r.area_radius as f32 == v.area_radius_m()
                && r.area_ceiling == alt(v.area_ceiling_raw)
                && r.area_floor == alt(v.area_floor_raw)
                && r.operator_altitude_geo == alt(v.op_alt_geo_raw)
                && check_opt(v.area_ceiling_m(), r.area_ceiling, -1000.0)
                && check_opt(v.area_floor_m(), r.area_floor, -1000.0)
                && check_opt(v.op_alt_geo_m(), r.operator_altitude_geo, -1000.0)
                && r.category_eu == v.category_eu.into()
                && r.class_eu == v.class_eu.into()
                && r.timestamp == v.timestamp
        }
        Message::OperatorId(v) => {
            let r = c::operator_id(m)?;
            r.operator_id_type == v.operator_id_type.into()
                && r.operator_id[..20] == nul_fill(&v.operator_id)
        }
    })
}

#[test]
fn messages_match_reference() {
    let mut rng = XorShift32(0x2545F491);
    let (mut mismatches, mut rejected) = (0u32, 0u32);
    for _ in 0..N {
        let mut m = [0u8; 25];
        m.iter_mut().for_each(|b| *b = rng.byte());
        m[0] = ((rng.next() % 6) as u8) << 4 | (m[0] & 0x0F);
        match compare(&m) {
            Some(true) => {}
            Some(false) => {
                if mismatches < 5 {
                    eprintln!("mismatch: {m:02x?}");
                }
                mismatches += 1;
            }
            None => rejected += 1,
        }
    }
    eprintln!("messages={N} mismatches={mismatches} rejected_by_c={rejected}");
    assert_eq!(mismatches, 0);
}

#[test]
fn packs_match_reference() {
    let mut rng = XorShift32(0x2545F491);
    let mut mismatches = 0u32;
    let mut accepted = 0u32;
    for _ in 0..N {
        let mut p = [0u8; 228];
        p.iter_mut().for_each(|b| *b = rng.byte());
        // Mostly well-formed headers, so the content rule is exercised rather than the header checks.
        if !rng.next().is_multiple_of(16) {
            p[0] |= 0xF0;
        }
        if !rng.next().is_multiple_of(16) {
            p[1] = 25;
        }
        p[2] = (rng.next() % 11) as u8;
        for i in 0..9 {
            let ty = [0, 0, 1, 2, 2, 3, 4, 5, 6, 0xF][(rng.next() % 10) as usize];
            p[3 + 25 * i] = ty << 4 | (p[3 + 25 * i] & 0x0F);
        }
        let ours = parse_pack(&p).is_ok();
        accepted += u32::from(ours);
        if ours != c::pack_ok(&p) {
            if mismatches < 5 {
                eprintln!("pack mismatch (ours ok={ours}): {:02x?}", &p[..3]);
            }
            mismatches += 1;
        }
    }
    eprintln!("packs={N} mismatches={mismatches} accepted={accepted}");
    assert_eq!(mismatches, 0);
    assert!(accepted > N / 20, "too few valid packs to be meaningful");
}
