#![forbid(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use odid::Message;
use rid_proto::{Frame, FrameError, Payload};
use serde::Serialize;

#[derive(Parser)]
#[command(about = "Decode the rid-pocket USB stream to JSON Lines")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Read a serial port forever (/dev/ttyACM0 or COM5).
    Live {
        #[arg(long)]
        port: String,
        #[arg(long)]
        out: Option<PathBuf>,
        /// Append every received byte to this file for later replay.
        #[arg(long)]
        raw: Option<PathBuf>,
        #[arg(long, default_value_t = 10)]
        stats_every: u64,
    },
    /// Decode a capture file and exit at EOF.
    Decode {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Default)]
struct Stats {
    frames: u64,
    hello: u64,
    obs: u64,
    status: u64,
    log: u64,
    bad_crc: u64,
    bad_frames: u64,
    text_chunks: u64,
    unknown: u64,
    seq_gaps: u64,
    seq_reorder: u64,
}

impl Stats {
    fn print(&self) {
        eprintln!(
            "stats frames={} hello={} obs={} status={} log={} bad_crc={} bad_frames={} text_chunks={} unknown={} seq_gaps={} seq_reorder={}",
            self.frames,
            self.hello,
            self.obs,
            self.status,
            self.log,
            self.bad_crc,
            self.bad_frames,
            self.text_chunks,
            self.unknown,
            self.seq_gaps,
            self.seq_reorder
        );
    }
}

#[derive(Serialize)]
struct Line {
    r#type: &'static str,
    seq: u32,
    t_dev_us: u64,
    host_rx_unix_ns: u128,
    #[serde(flatten)]
    body: Body,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Body {
    Hello {
        fw_version: String,
        git_sha: String,
        device_mac: String,
        boot_channel: u8,
    },
    Obs {
        channel: u8,
        rssi_dbm: i8,
        source: &'static str,
        src_mac: String,
        msg_counter: u8,
        t_mac_us: u32,
        ie_len: u8,
        pack_hex: String,
        pack_error: Option<String>,
        messages: Vec<Msg>,
    },
    Status {
        uptime_s: u32,
        channel: u8,
        mode: u8,
        tracks_active: u8,
        heap_free: u32,
        mgmt_frames: u32,
        beacons: u32,
        rid_frames: u32,
        beacon_errors: u32,
        pack_errors: u32,
        obs_dropped: u32,
        tx_dropped: u32,
        radio_errors: u32,
        log_dropped: u32,
    },
    Log {
        level: &'static str,
        text: String,
    },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Msg {
    BasicId {
        id_type: u8,
        ua_type: u8,
        uas_id: String,
    },
    Location {
        status: u8,
        height_type: u8,
        track_deg: Option<u16>,
        speed_h_mps: Option<f32>,
        speed_v_mps: Option<f32>,
        lat: Option<f64>,
        lon: Option<f64>,
        alt_baro_m: Option<f32>,
        alt_geo_m: Option<f32>,
        height_m: Option<f32>,
        horiz_acc: u8,
        vert_acc: u8,
        baro_acc: u8,
        speed_acc: u8,
        timestamp_s: Option<f32>,
        ts_acc: u8,
    },
    Auth {
        auth_type: u8,
        data_page: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_page_index: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        length: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_unix_s: Option<u64>,
    },
    SelfId {
        desc_type: u8,
        desc: String,
    },
    System {
        operator_location_type: u8,
        classification_type: u8,
        op_lat: Option<f64>,
        op_lon: Option<f64>,
        op_alt_geo_m: Option<f32>,
        area_count: u16,
        area_radius_m: f32,
        area_ceiling_m: Option<f32>,
        area_floor_m: Option<f32>,
        category_eu: u8,
        class_eu: u8,
        timestamp_unix_s: u64,
    },
    OperatorId {
        operator_id_type: u8,
        operator_id: String,
    },
}

fn text(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

fn mac(m: &[u8; 6]) -> String {
    m.map(|b| format!("{b:02x}")).join(":")
}

fn to_msg(m: Message) -> Msg {
    match m {
        Message::BasicId(v) => Msg::BasicId {
            id_type: v.id_type,
            ua_type: v.ua_type,
            uas_id: text(&v.uas_id),
        },
        Message::Location(v) => Msg::Location {
            status: v.status,
            height_type: v.height_type,
            track_deg: v.track_deg(),
            speed_h_mps: v.speed_h_mps(),
            speed_v_mps: v.speed_v_mps(),
            lat: v.latitude_deg(),
            lon: v.longitude_deg(),
            alt_baro_m: v.alt_baro_m(),
            alt_geo_m: v.alt_geo_m(),
            height_m: v.height_m(),
            horiz_acc: v.horiz_acc,
            vert_acc: v.vert_acc,
            baro_acc: v.baro_acc,
            speed_acc: v.speed_acc,
            timestamp_s: v.timestamp_s(),
            ts_acc: v.ts_acc,
        },
        Message::Auth(v) => Msg::Auth {
            auth_type: v.auth_type,
            data_page: v.data_page,
            last_page_index: v.last_page_index(),
            length: v.length(),
            timestamp_unix_s: v.timestamp_unix_s(),
        },
        Message::SelfId(v) => Msg::SelfId {
            desc_type: v.desc_type,
            desc: text(&v.desc),
        },
        Message::System(v) => Msg::System {
            operator_location_type: v.operator_location_type,
            classification_type: v.classification_type,
            op_lat: v.latitude_deg(),
            op_lon: v.longitude_deg(),
            op_alt_geo_m: v.op_alt_geo_m(),
            area_count: v.area_count,
            area_radius_m: v.area_radius_m(),
            area_ceiling_m: v.area_ceiling_m(),
            area_floor_m: v.area_floor_m(),
            category_eu: v.category_eu,
            class_eu: v.class_eu,
            timestamp_unix_s: v.timestamp_unix_s(),
        },
        Message::OperatorId(v) => Msg::OperatorId {
            operator_id_type: v.operator_id_type,
            operator_id: text(&v.operator_id),
        },
    }
}

fn body(p: Payload) -> (&'static str, Body) {
    match p {
        Payload::Hello(h) => (
            "hello",
            Body::Hello {
                fw_version: text(&h.fw_version),
                git_sha: text(&h.git_sha),
                device_mac: mac(&h.device_mac),
                boot_channel: h.boot_channel,
            },
        ),
        Payload::Obs(o) => {
            let (pack_error, messages) = match odid::parse_pack(o.pack) {
                Ok(it) => (
                    None,
                    it.filter_map(|m| odid::decode_message(m).ok())
                        .map(to_msg)
                        .collect(),
                ),
                Err(e) => {
                    let name = format!("{e:?}");
                    (
                        Some(name.split('(').next().unwrap_or("").to_owned()),
                        Vec::new(),
                    )
                }
            };
            (
                "obs",
                Body::Obs {
                    channel: o.channel,
                    rssi_dbm: o.rssi_dbm,
                    source: if o.source == 1 {
                        "wifi_beacon"
                    } else {
                        "unknown"
                    },
                    src_mac: mac(&o.src_mac),
                    msg_counter: o.msg_counter,
                    t_mac_us: o.t_mac_us,
                    ie_len: o.ie_len,
                    pack_hex: o.pack.iter().map(|b| format!("{b:02x}")).collect(),
                    pack_error,
                    messages,
                },
            )
        }
        Payload::Status(s) => {
            let [
                mgmt_frames,
                beacons,
                rid_frames,
                beacon_errors,
                pack_errors,
                obs_dropped,
                tx_dropped,
                radio_errors,
                log_dropped,
            ] = s.counters;
            (
                "status",
                Body::Status {
                    uptime_s: s.uptime_s,
                    channel: s.channel,
                    mode: s.mode,
                    tracks_active: s.tracks_active,
                    heap_free: s.heap_free,
                    mgmt_frames,
                    beacons,
                    rid_frames,
                    beacon_errors,
                    pack_errors,
                    obs_dropped,
                    tx_dropped,
                    radio_errors,
                    log_dropped,
                },
            )
        }
        Payload::Log(l) => (
            "log",
            Body::Log {
                level: match l.level {
                    1 => "error",
                    2 => "warn",
                    3 => "info",
                    4 => "debug",
                    5 => "trace",
                    _ => "unknown",
                },
                text: text(l.text),
            },
        ),
    }
}

struct Decoder<W: Write> {
    chunk: Vec<u8>,
    prev: Option<u32>,
    stats: Stats,
    out: W,
}

impl<W: Write> Decoder<W> {
    fn feed(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        for &b in bytes {
            if b != 0 {
                self.chunk.push(b);
            } else if !self.chunk.is_empty() {
                let chunk = std::mem::take(&mut self.chunk);
                self.frame(&chunk)?;
            }
        }
        Ok(())
    }

    fn frame(&mut self, chunk: &[u8]) -> anyhow::Result<()> {
        let rx_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let device_text = || {
            let t = String::from_utf8_lossy(chunk);
            eprintln!("[device] {}", t.trim_end_matches(['\r', '\n']));
        };
        let mut raw = vec![0; chunk.len()];
        let Some(n) = rid_proto::cobs_decode(chunk, &mut raw) else {
            self.stats.text_chunks += 1;
            device_text();
            return Ok(());
        };
        if n < rid_proto::HEADER_LEN + 4 {
            self.stats.bad_frames += 1;
            device_text();
            return Ok(());
        }
        let Frame {
            seq,
            t_dev_us,
            payload,
        } = match rid_proto::decode_raw(&raw[..n]) {
            Ok(f) => f,
            Err(FrameError::BadCrc) => {
                self.stats.bad_crc += 1;
                return Ok(());
            }
            Err(FrameError::BadLength) => {
                self.stats.bad_frames += 1;
                return Ok(());
            }
            Err(FrameError::Unknown) => {
                self.stats.unknown += 1;
                return Ok(());
            }
        };
        match (&payload, self.prev) {
            (Payload::Hello(_), _) if seq == 0 => self.prev = Some(0),
            (_, None) => self.prev = Some(seq),
            (_, Some(prev)) => {
                let d = seq.wrapping_sub(prev);
                if (1..1 << 31).contains(&d) {
                    self.stats.seq_gaps += u64::from(d - 1);
                    self.prev = Some(seq);
                } else {
                    self.stats.seq_reorder += 1;
                }
            }
        }
        self.stats.frames += 1;
        let (ty, body) = body(payload);
        *match ty {
            "hello" => &mut self.stats.hello,
            "obs" => &mut self.stats.obs,
            "status" => &mut self.stats.status,
            _ => &mut self.stats.log,
        } += 1;
        let line = Line {
            r#type: ty,
            seq,
            t_dev_us,
            host_rx_unix_ns: rx_ns,
            body,
        };
        serde_json::to_writer(&mut self.out, &line)?;
        self.out.write_all(b"\n")?;
        self.out.flush()?;
        Ok(())
    }
}

fn output(path: Option<PathBuf>) -> io::Result<Box<dyn Write>> {
    Ok(match path {
        Some(p) => Box::new(File::create(p)?),
        None => Box::new(io::stdout().lock()),
    })
}

#[cfg(windows)]
fn open_port(port: &str) -> io::Result<File> {
    let path = if port.starts_with(r"\\.\") {
        port.to_owned()
    } else {
        format!(r"\\.\{port}")
    };
    OpenOptions::new().read(true).write(true).open(path)
}

#[cfg(unix)]
fn open_port(port: &str) -> io::Result<File> {
    use rustix::fs::{Mode, OFlags};
    use rustix::termios::{OptionalActions, tcgetattr, tcsetattr};
    let fd = rustix::fs::open(
        port,
        OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let mut t = tcgetattr(&fd)?;
    t.make_raw();
    tcsetattr(&fd, OptionalActions::Now, &t)?;
    Ok(File::from(fd))
}

// usbser's zero timeouts make a read wait until its buffer is full, so read one byte at a time.
#[cfg(windows)]
const READ_CHUNK: usize = 1;
#[cfg(unix)]
const READ_CHUNK: usize = 4096;

fn run(cli: Cli) -> Result<(), (u8, anyhow::Error)> {
    let open = |e: io::Error| (2, e.into());
    match cli.cmd {
        Cmd::Decode { file, out } => {
            let mut bytes = Vec::new();
            File::open(file)
                .map_err(open)?
                .read_to_end(&mut bytes)
                .map_err(|e| (3, e.into()))?;
            let mut d = Decoder {
                chunk: Vec::new(),
                prev: None,
                stats: Stats::default(),
                out: output(out).map_err(open)?,
            };
            d.feed(&bytes).map_err(|e| (3, e))?;
            d.stats.print();
            Ok(())
        }
        Cmd::Live {
            port,
            out,
            raw,
            stats_every,
        } => {
            let mut port = open_port(&port).map_err(open)?;
            let mut raw = match raw {
                Some(p) => Some(
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(p)
                        .map_err(open)?,
                ),
                None => None,
            };
            let mut d = Decoder {
                chunk: Vec::new(),
                prev: None,
                stats: Stats::default(),
                out: output(out).map_err(open)?,
            };
            let every = Duration::from_secs(stats_every);
            let mut last = Instant::now();
            let mut buf = [0u8; READ_CHUNK];
            loop {
                // Ok(0) means no data yet, not EOF.
                let n = port.read(&mut buf).map_err(|e| (3, e.into()))?;
                if let Some(f) = raw.as_mut() {
                    f.write_all(&buf[..n]).map_err(|e| (3, e.into()))?;
                }
                d.feed(&buf[..n]).map_err(|e| (3, e))?;
                if last.elapsed() >= every {
                    d.stats.print();
                    last = Instant::now();
                }
            }
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, e)) => {
            eprintln!("error: {e:#}");
            ExitCode::from(code)
        }
    }
}
