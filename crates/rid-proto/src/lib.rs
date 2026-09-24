#![no_std]
#![forbid(unsafe_code)]

pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 14;
pub const PACK_CAP: usize = 228;
pub const MAX_RAW: usize = HEADER_LEN + 16 + PACK_CAP + 4;
pub const LOG_TEXT_CAP: usize = 200;

pub const TYPE_HELLO: u8 = 0x01;
pub const TYPE_OBS: u8 = 0x02;
pub const TYPE_STATUS: u8 = 0x03;
pub const TYPE_LOG: u8 = 0x04;

const CRC32: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);

pub fn crc32(data: &[u8]) -> u32 {
    CRC32.checksum(data)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hello {
    pub fw_version: [u8; 16],
    pub git_sha: [u8; 8],
    pub device_mac: [u8; 6],
    pub boot_channel: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Obs<'a> {
    pub channel: u8,
    pub rssi_dbm: i8,
    pub source: u8,
    pub src_mac: [u8; 6],
    pub msg_counter: u8,
    pub t_mac_us: u32,
    pub ie_len: u8,
    pub pack: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub uptime_s: u32,
    pub channel: u8,
    pub mode: u8,
    pub tracks_active: u8,
    pub heap_free: u32,
    /// Section 6.4 order: mgmt, beacons, rid, beacon_err, pack_err, obs_drop, tx_drop, radio_err, log_drop.
    pub counters: [u32; 9],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Log<'a> {
    pub level: u8,
    pub text: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payload<'a> {
    Hello(Hello),
    Obs(Obs<'a>),
    Status(Status),
    Log(Log<'a>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame<'a> {
    pub seq: u32,
    pub t_dev_us: u64,
    pub payload: Payload<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// Shorter than header + CRC, or payload length disagrees with its type.
    BadLength,
    BadCrc,
    /// `version` other than 1, or a type this decoder does not know.
    Unknown,
}

pub struct FrameBuf {
    pub len: u16,
    pub bytes: [u8; 320],
}

struct Writer<'a> {
    buf: &'a mut [u8],
    n: usize,
}

impl Writer<'_> {
    fn put(&mut self, b: &[u8]) -> Option<()> {
        let end = self.n.checked_add(b.len())?;
        self.buf.get_mut(self.n..end)?.copy_from_slice(b);
        self.n = end;
        Some(())
    }
}

/// Writes the raw frame (header, payload, CRC) into `out`; returns its length.
pub fn encode_raw(frame: &Frame, out: &mut [u8]) -> Option<usize> {
    let mut w = Writer { buf: out, n: 0 };
    let ty = match frame.payload {
        Payload::Hello(_) => TYPE_HELLO,
        Payload::Obs(_) => TYPE_OBS,
        Payload::Status(_) => TYPE_STATUS,
        Payload::Log(_) => TYPE_LOG,
    };
    w.put(&[VERSION, ty])?;
    w.put(&frame.seq.to_le_bytes())?;
    w.put(&frame.t_dev_us.to_le_bytes())?;
    match &frame.payload {
        Payload::Hello(h) => {
            w.put(&h.fw_version)?;
            w.put(&h.git_sha)?;
            w.put(&h.device_mac)?;
            w.put(&[h.boot_channel])?;
        }
        Payload::Obs(o) => {
            let pack_len = u8::try_from(o.pack.len())
                .ok()
                .filter(|&l| usize::from(l) <= PACK_CAP)?;
            w.put(&[o.channel, o.rssi_dbm as u8, o.source])?;
            w.put(&o.src_mac)?;
            w.put(&[o.msg_counter])?;
            w.put(&o.t_mac_us.to_le_bytes())?;
            w.put(&[o.ie_len, pack_len])?;
            w.put(o.pack)?;
        }
        Payload::Status(s) => {
            w.put(&s.uptime_s.to_le_bytes())?;
            w.put(&[s.channel, s.mode, s.tracks_active, 0])?;
            w.put(&s.heap_free.to_le_bytes())?;
            for c in s.counters {
                w.put(&c.to_le_bytes())?;
            }
        }
        Payload::Log(l) => {
            let len = u8::try_from(l.text.len())
                .ok()
                .filter(|&l| usize::from(l) <= LOG_TEXT_CAP)?;
            w.put(&[l.level, len])?;
            w.put(l.text)?;
        }
    }
    let crc = crc32(w.buf.get(..w.n)?);
    w.put(&crc.to_le_bytes())?;
    Some(w.n)
}

impl FrameBuf {
    /// COBS-encoded wire frame including the trailing `0x00`. `None` if a field exceeds its cap.
    pub fn encode(frame: &Frame) -> Option<FrameBuf> {
        let mut raw = [0u8; MAX_RAW];
        let n = encode_raw(frame, &mut raw)?;
        let mut fb = FrameBuf {
            len: 0,
            bytes: [0; 320],
        };
        let c = cobs_encode(raw.get(..n)?, &mut fb.bytes)?;
        *fb.bytes.get_mut(c)? = 0;
        fb.len = u16::try_from(c.checked_add(1)?).ok()?;
        Some(fb)
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.len)).unwrap_or(&[])
    }
}

fn arr<const N: usize>(b: &[u8], at: usize) -> Option<[u8; N]> {
    b.get(at..)?.first_chunk::<N>().copied()
}

/// Decodes one raw (already COBS-decoded) frame.
pub fn decode_raw(raw: &[u8]) -> Result<Frame<'_>, FrameError> {
    let (body, crc) = raw
        .split_last_chunk::<4>()
        .filter(|(b, _)| b.len() >= HEADER_LEN)
        .ok_or(FrameError::BadLength)?;
    if crc32(body) != u32::from_le_bytes(*crc) {
        return Err(FrameError::BadCrc);
    }
    let (h, p) = body.split_at(HEADER_LEN);
    let seq = u32::from_le_bytes(arr(h, 2).ok_or(FrameError::BadLength)?);
    let t_dev_us = u64::from_le_bytes(arr(h, 6).ok_or(FrameError::BadLength)?);
    if h.first() != Some(&VERSION) {
        return Err(FrameError::Unknown);
    }
    let u32_at = |i| arr(p, i).map(u32::from_le_bytes);
    let payload = match h.get(1) {
        Some(&TYPE_HELLO) if p.len() == 31 => Payload::Hello(Hello {
            fw_version: arr(p, 0).ok_or(FrameError::BadLength)?,
            git_sha: arr(p, 16).ok_or(FrameError::BadLength)?,
            device_mac: arr(p, 24).ok_or(FrameError::BadLength)?,
            boot_channel: p[30],
        }),
        Some(&TYPE_OBS)
            if p.len() >= 16
                && p.len() == 16 + usize::from(p[15])
                && usize::from(p[15]) <= PACK_CAP =>
        {
            Payload::Obs(Obs {
                channel: p[0],
                rssi_dbm: p[1] as i8,
                source: p[2],
                src_mac: arr(p, 3).ok_or(FrameError::BadLength)?,
                msg_counter: p[9],
                t_mac_us: u32_at(10).ok_or(FrameError::BadLength)?,
                ie_len: p[14],
                pack: &p[16..],
            })
        }
        Some(&TYPE_STATUS) if p.len() == 48 => {
            let mut counters = [0; 9];
            for (i, c) in counters.iter_mut().enumerate() {
                *c = u32_at(12 + 4 * i).ok_or(FrameError::BadLength)?;
            }
            Payload::Status(Status {
                uptime_s: u32_at(0).ok_or(FrameError::BadLength)?,
                channel: p[4],
                mode: p[5],
                tracks_active: p[6],
                heap_free: u32_at(8).ok_or(FrameError::BadLength)?,
                counters,
            })
        }
        Some(&TYPE_LOG)
            if p.len() >= 2
                && p.len() == 2 + usize::from(p[1])
                && usize::from(p[1]) <= LOG_TEXT_CAP =>
        {
            Payload::Log(Log {
                level: p[0],
                text: &p[2..],
            })
        }
        Some(&(TYPE_HELLO..=TYPE_LOG)) => return Err(FrameError::BadLength),
        _ => return Err(FrameError::Unknown),
    };
    Ok(Frame {
        seq,
        t_dev_us,
        payload,
    })
}

/// Cheshire–Baker COBS without the trailing delimiter. Returns the encoded length.
pub fn cobs_encode(src: &[u8], dst: &mut [u8]) -> Option<usize> {
    let mut code_at = 0;
    let mut n = 1;
    let mut code = 1u8;
    for &b in src {
        if b == 0 {
            *dst.get_mut(code_at)? = code;
            code_at = n;
            n += 1;
            code = 1;
        } else {
            *dst.get_mut(n)? = b;
            n += 1;
            code += 1;
            if code == 0xFF {
                *dst.get_mut(code_at)? = code;
                code_at = n;
                n += 1;
                code = 1;
            }
        }
    }
    *dst.get_mut(code_at)? = code;
    Some(n)
}

/// Accepts both the form with and without a trailing `0x01` after a full 254-byte run.
pub fn cobs_decode(src: &[u8], dst: &mut [u8]) -> Option<usize> {
    let mut i = 0;
    let mut n = 0;
    while i < src.len() {
        let code = usize::from(src[i]);
        if code == 0 {
            return None;
        }
        let run = src.get(i + 1..i + code)?;
        if run.contains(&0) {
            return None;
        }
        dst.get_mut(n..n + run.len())?.copy_from_slice(run);
        n += run.len();
        i += code;
        if code < 0xFF && i < src.len() {
            *dst.get_mut(n)? = 0;
            n += 1;
        }
    }
    Some(n)
}
