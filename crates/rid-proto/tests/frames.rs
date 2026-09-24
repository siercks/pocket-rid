use rid_proto::*;

fn hex(s: &str) -> Vec<u8> {
    let s = s.trim();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn cobs(src: &[u8]) -> Vec<u8> {
    let mut out = vec![0; src.len() + src.len() / 254 + 2];
    let n = cobs_encode(src, &mut out).unwrap();
    out.truncate(n);
    out
}

fn uncobs(src: &[u8]) -> Option<Vec<u8>> {
    let mut out = vec![0; src.len()];
    let n = cobs_decode(src, &mut out)?;
    out.truncate(n);
    Some(out)
}

#[test]
fn cobs_table() {
    let run254: Vec<u8> = (1..=254).collect();
    let run255: Vec<u8> = (1..=255).collect();
    let mut want254 = vec![0xFF];
    want254.extend(&run254);
    want254.push(0x01);
    let mut want255 = vec![0xFF];
    want255.extend(&run254);
    want255.extend([0x02, 0xFF]);
    let cases: [(&[u8], &[u8]); 5] = [
        (&[], &[0x01]),
        (&[0x00], &[0x01, 0x01]),
        (&[0x11, 0x22, 0x00, 0x33], &[0x03, 0x11, 0x22, 0x02, 0x33]),
        (&run254, &want254),
        (&run255, &want255),
    ];
    for (raw, enc) in cases {
        assert_eq!(cobs(raw), enc);
        assert_eq!(uncobs(enc).unwrap(), raw);
    }
    // Full 254-byte run without the trailing 0x01 code.
    assert_eq!(uncobs(&want254[..255]).unwrap(), run254);
    // Code byte pointing past the end of the chunk.
    assert_eq!(uncobs(&[0x05, 0x11, 0x22]), None);
}

#[test]
fn crc_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

fn padded<const N: usize>(s: &[u8]) -> [u8; N] {
    let mut a = [0; N];
    a[..s.len()].copy_from_slice(s);
    a
}

#[test]
fn golden_frames() {
    let pack = hex(include_str!("../../../testdata/pack_5msg.hex"));
    let frames = [
        (
            Frame {
                seq: 0,
                t_dev_us: 1_500_000,
                payload: Payload::Hello(Hello {
                    fw_version: padded(b"0.1.0"),
                    git_sha: *b"abcdef12",
                    device_mac: [0x24, 0x0a, 0xc4, 0, 0, 1],
                    boot_channel: 6,
                }),
            },
            49,
            0xf654bb42,
            "0301010101010460e3160101010106302e312e30010101010101010101010c6162636465663132240ac40107010642bb54f600",
        ),
        (
            Frame {
                seq: 1,
                t_dev_us: 1_600_000,
                payload: Payload::Log(Log {
                    level: 3,
                    text: b"radio up ch=6",
                }),
            },
            33,
            0x7de1619b,
            "04010401010101036a180101010114030d726164696f2075702063683d369b61e17d00",
        ),
        (
            Frame {
                seq: 2,
                t_dev_us: 42_000_000,
                payload: Payload::Status(Status {
                    uptime_s: 42,
                    channel: 6,
                    mode: 0,
                    tracks_active: 1,
                    heap_free: 40_000,
                    counters: [1000, 800, 12, 0, 1, 0, 0, 0, 0],
                }),
            },
            66,
            0x2856589e,
            "0401030201010580de8002010101022a01010206020103409c0103e8030103200301020c0101010101010201010101010101010101010101010101010101059e58562800",
        ),
        (
            Frame {
                seq: 3,
                t_dev_us: 42_123_456,
                payload: Payload::Obs(Obs {
                    channel: 6,
                    rssi_dbm: -67,
                    source: 1,
                    src_mac: [0x02, 0x11, 0x22, 0x33, 0x44, 0x55],
                    msg_counter: 0x2A,
                    t_mac_us: 9_999_999,
                    ie_len: 133,
                    pack: &pack,
                }),
            },
            162,
            0x779e639b,
            "04010203010105c0c082020101010e06bd010211223344552a7f96981b8580f219050212524944504f434b45542d544553542d303030310101011912205a14028074d21ac0282db8f308fc0834084a3339300202320b42656e636820746573740101010101010101010101010c4201209af81a200307b80101010101010107980880ba890e02520d4f502d544553542d3030303101010101010101010101059b639e7700",
        ),
    ];
    for (frame, raw_len, crc, wire) in frames {
        let mut raw = [0; MAX_RAW];
        let n = encode_raw(&frame, &mut raw).unwrap();
        assert_eq!(n, raw_len);
        assert_eq!(u32::from_le_bytes(raw[n - 4..n].try_into().unwrap()), crc);
        let fb = FrameBuf::encode(&frame).unwrap();
        assert_eq!(fb.as_bytes(), hex(wire).as_slice());
        assert_eq!(decode_raw(&raw[..n]).unwrap(), frame);
    }
}

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
    fn bytes<const N: usize>(&mut self) -> [u8; N] {
        std::array::from_fn(|_| self.byte())
    }
}

#[test]
fn random_round_trips() {
    let mut rng = XorShift32(0x2545F491);
    for _ in 0..10_000 {
        let blob: Vec<u8> = (0..rng.next() % 229).map(|_| rng.byte()).collect();
        let payload = match rng.next() % 4 {
            0 => Payload::Hello(Hello {
                fw_version: rng.bytes(),
                git_sha: rng.bytes(),
                device_mac: rng.bytes(),
                boot_channel: rng.byte(),
            }),
            1 => Payload::Obs(Obs {
                channel: rng.byte(),
                rssi_dbm: rng.byte() as i8,
                source: rng.byte(),
                src_mac: rng.bytes(),
                msg_counter: rng.byte(),
                t_mac_us: rng.next(),
                ie_len: rng.byte(),
                pack: &blob,
            }),
            2 => Payload::Status(Status {
                uptime_s: rng.next(),
                channel: rng.byte(),
                mode: rng.byte(),
                tracks_active: rng.byte(),
                heap_free: rng.next(),
                counters: std::array::from_fn(|_| rng.next()),
            }),
            _ => Payload::Log(Log {
                level: rng.byte(),
                text: &blob[..blob.len().min(LOG_TEXT_CAP)],
            }),
        };
        let frame = Frame {
            seq: rng.next(),
            t_dev_us: (u64::from(rng.next()) << 32) | u64::from(rng.next()),
            payload,
        };
        let fb = FrameBuf::encode(&frame).unwrap();
        let (last, body) = fb.as_bytes().split_last().unwrap();
        assert_eq!(*last, 0);
        assert!(!body.contains(&0));
        let raw = uncobs(body).unwrap();
        assert_eq!(decode_raw(&raw).unwrap(), frame);
    }
}
