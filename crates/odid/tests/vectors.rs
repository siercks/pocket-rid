use odid::*;

fn hex(s: &str) -> Vec<u8> {
    let s = s.trim();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn msg(s: &str) -> [u8; 25] {
    hex(s).try_into().unwrap()
}

const BEACON: &str = include_str!("../../../testdata/beacon_5msg.hex");
const PACK: &str = include_str!("../../../testdata/pack_5msg.hex");
const BASIC_ID: &str = include_str!("../../../testdata/msg_basic_id.hex");
const LOCATION: &str = include_str!("../../../testdata/msg_location.hex");
const SELF_ID: &str = include_str!("../../../testdata/msg_self_id.hex");
const SYSTEM: &str = include_str!("../../../testdata/msg_system.hex");
const OPERATOR_ID: &str = include_str!("../../../testdata/msg_operator_id.hex");
const AUTH_P0: &str = include_str!("../../../testdata/msg_auth_p0.hex");

fn location(edit: impl FnOnce(&mut [u8; 25])) -> Location {
    let mut m = msg(LOCATION);
    edit(&mut m);
    match decode_message(&m).unwrap() {
        Message::Location(l) => l,
        other => panic!("{other:?}"),
    }
}

#[test]
fn beacon_positive() {
    let b = hex(BEACON);
    let r = parse_beacon(&b).unwrap();
    assert_eq!(r.src_mac, [0x02, 0x11, 0x22, 0x33, 0x44, 0x55]);
    assert_eq!(r.msg_counter, 0x2A);
    assert_eq!(r.pack, &b[56..184]);
    assert_eq!(r.pack, hex(PACK).as_slice());
}

#[test]
fn beacon_negative() {
    let b = hex(BEACON);
    let edit = |i: usize, v: u8| {
        let mut b = b.clone();
        b[i] = v;
        parse_beacon(&b).map(|_| ())
    };
    assert_eq!(
        parse_beacon(&b[..35]).map(|_| ()),
        Err(BeaconError::TooShort)
    );
    assert_eq!(edit(0, 0x50), Err(BeaconError::NotBeacon));
    assert_eq!(
        parse_beacon(&b[..36]).map(|_| ()),
        Err(BeaconError::NotRemoteId)
    );
    assert_eq!(edit(50, 0x86), Err(BeaconError::TruncatedIe));
    assert_eq!(edit(54, 0x0e), Err(BeaconError::NotRemoteId));
}

#[test]
fn pack_positive() {
    let p = hex(PACK);
    let msgs: Vec<_> = parse_pack(&p).unwrap().copied().collect();
    let want = [BASIC_ID, LOCATION, SELF_ID, SYSTEM, OPERATOR_ID].map(msg);
    assert_eq!(msgs, want);
    let mut out = [0; 256];
    let n = build_pack(&want, &mut out).unwrap();
    assert_eq!(&out[..n], p.as_slice());
    let mut beacon = [0; 256];
    let n = build_beacon([2, 0x11, 0x22, 0x33, 0x44, 0x55], 0x2A, &p, &mut beacon).unwrap();
    let r = parse_beacon(&beacon[..n]).unwrap();
    assert_eq!((r.msg_counter, r.pack), (0x2A, p.as_slice()));
}

#[test]
fn pack_negative() {
    let p = hex(PACK);
    let edit = |i: usize, v: u8| {
        let mut p = p.clone();
        p[i] = v;
        parse_pack(&p).map(|_| ()).unwrap_err()
    };
    assert_eq!(
        parse_pack(&p[..2]).map(|_| ()).unwrap_err(),
        PackError::TooShort
    );
    assert_eq!(edit(0, 0xe2), PackError::NotAPack);
    assert_eq!(edit(1, 0x18), PackError::BadMessageSize(24));
    assert_eq!(edit(2, 0x0a), PackError::TooManyMessages(10));
    assert_eq!(edit(2, 0x00), PackError::Empty);
    assert_eq!(edit(2, 0x06), PackError::Truncated);
    assert_eq!(edit(53, 0x12), PackError::InvalidContent);
    assert_eq!(edit(53, 0x62), PackError::InvalidContent);
    assert_eq!(edit(53, 0xf2), PackError::InvalidContent);
}

#[test]
fn basic_id() {
    let m = msg(BASIC_ID);
    let Message::BasicId(v) = decode_message(&m).unwrap() else {
        panic!()
    };
    assert_eq!((v.proto_version, v.id_type, v.ua_type), (2, 1, 2));
    assert_eq!(v.as_str(), "RIDPOCKET-TEST-0001");
    assert_eq!(encode_basic_id(&v), m);
}

#[test]
fn location_positive() {
    let v = location(|_| ());
    assert_eq!(
        (v.status, v.height_type, v.ew_direction, v.speed_mult),
        (2, 0, 0, 0)
    );
    assert_eq!((v.direction_raw, v.speed_h_raw, v.speed_v_raw), (90, 20, 2));
    assert_eq!((v.lat_e7, v.lon_e7), (450_000_000, -1_205_000_000));
    assert_eq!(
        (v.alt_baro_raw, v.alt_geo_raw, v.height_raw),
        (2291, 2300, 2100)
    );
    assert_eq!(
        (v.horiz_acc, v.vert_acc, v.baro_acc, v.speed_acc),
        (10, 4, 3, 3)
    );
    assert_eq!((v.timestamp_raw, v.ts_acc), (12345, 2));
    assert_eq!(v.track_deg(), Some(90));
    assert_eq!(v.speed_h_mps(), Some(5.0));
    assert_eq!(v.speed_v_mps(), Some(1.0));
    assert!((v.latitude_deg().unwrap() - 45.0).abs() < 1e-9);
    assert!((v.longitude_deg().unwrap() + 120.5).abs() < 1e-9);
    assert_eq!(v.alt_baro_m(), Some(145.5));
    assert_eq!(v.alt_geo_m(), Some(150.0));
    assert_eq!(v.height_m(), Some(50.0));
    assert!((v.timestamp_s().unwrap() - 1234.5).abs() < 1e-3);
    assert_eq!(encode_location(&v), msg(LOCATION));
}

#[test]
fn location_invalid_values() {
    let v = location(|m| m[5..13].fill(0));
    assert_eq!((v.latitude_deg(), v.longitude_deg()), (None, None));
    assert_eq!(location(|m| m[15..17].fill(0)).alt_geo_m(), None);
    assert_eq!(location(|m| (m[1], m[2]) = (0x22, 0xb5)).track_deg(), None);
    assert_eq!(
        location(|m| (m[1], m[3]) = (0x21, 0xff)).speed_h_mps(),
        None
    );
    assert_eq!(location(|m| m[4] = 0x7e).speed_v_mps(), None);
    assert_eq!(location(|m| m[21..23].fill(0xff)).timestamp_s(), None);
}

#[test]
fn decode_errors() {
    let mut m = msg(LOCATION);
    m[0] = 0xf2;
    assert_eq!(decode_message(&m), Err(DecodeError::NestedPack));
    m[0] = 0x72;
    assert_eq!(decode_message(&m), Err(DecodeError::UnknownType(7)));
}

#[test]
fn self_id() {
    let m = msg(SELF_ID);
    let Message::SelfId(v) = decode_message(&m).unwrap() else {
        panic!()
    };
    assert_eq!((v.desc_type, v.as_str()), (0, "Bench test"));
    assert_eq!(encode_self_id(&v), m);
}

#[test]
fn system() {
    let m = msg(SYSTEM);
    let Message::System(v) = decode_message(&m).unwrap() else {
        panic!()
    };
    assert_eq!((v.operator_location_type, v.classification_type), (1, 0));
    assert_eq!((v.op_lat_e7, v.op_lon_e7), (452_500_000, -1_207_500_000));
    assert_eq!(
        (
            v.area_count,
            v.area_radius_raw,
            v.area_ceiling_raw,
            v.area_floor_raw
        ),
        (1, 0, 0, 0)
    );
    assert_eq!(
        (v.category_eu, v.class_eu, v.op_alt_geo_raw, v.timestamp),
        (0, 0, 2200, 243_907_200)
    );
    assert!((v.latitude_deg().unwrap() - 45.25).abs() < 1e-9);
    assert!((v.longitude_deg().unwrap() + 120.75).abs() < 1e-9);
    assert_eq!(v.area_radius_m(), 0.0);
    assert_eq!((v.area_ceiling_m(), v.area_floor_m()), (None, None));
    assert_eq!(v.op_alt_geo_m(), Some(100.0));
    assert_eq!(v.timestamp_unix_s(), 1_790_208_000);
    assert_eq!(encode_system(&v), m);
}

#[test]
fn operator_id() {
    let m = msg(OPERATOR_ID);
    let Message::OperatorId(v) = decode_message(&m).unwrap() else {
        panic!()
    };
    assert_eq!((v.operator_id_type, v.as_str()), (0, "OP-TEST-0001"));
    assert_eq!(encode_operator_id(&v), m);
}

#[test]
fn auth_page0() {
    let m = msg(AUTH_P0);
    let Message::Auth(v) = decode_message(&m).unwrap() else {
        panic!()
    };
    assert_eq!((v.auth_type, v.data_page), (1, 0));
    assert_eq!((v.last_page_index(), v.length()), (Some(0), Some(17)));
    assert_eq!(v.timestamp_unix_s(), Some(243_907_200 + ODID_EPOCH_UNIX_S));
    assert_eq!(encode_auth(&v), m);
}
