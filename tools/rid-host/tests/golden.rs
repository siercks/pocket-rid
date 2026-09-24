use std::process::Command;

use serde_json::Value;

fn close(v: &Value, want: f64, tol: f64) {
    let got = v.as_f64().unwrap_or(f64::NAN);
    assert!((got - want).abs() <= tol, "{got} != {want}");
}

#[test]
fn golden_stream() {
    let out = Command::new(env!("CARGO_BIN_EXE_rid-host"))
        .args(["decode", "--file"])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/golden_stream.bin"
        ))
        .output()
        .unwrap();
    assert!(out.status.success());

    let stderr = String::from_utf8(out.stderr).unwrap();
    let err: Vec<&str> = stderr.lines().collect();
    assert_eq!(err.len(), 2, "{stderr}");
    assert_eq!(err[0], "[device] PANIC: test");
    let stats = err[1];
    for want in [
        "frames=4 ",
        "bad_crc=0 ",
        "seq_gaps=0 ",
        "seq_reorder=0",
        "text_chunks=1 ",
    ] {
        assert!(
            stats.starts_with("stats ") && stats.contains(want),
            "{stats}"
        );
    }

    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<Value> = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 4);
    for (i, (line, ty)) in stdout
        .lines()
        .zip(["hello", "log", "status", "obs"])
        .enumerate()
    {
        let prefix = format!(r#"{{"type":"{ty}","seq":{i},"t_dev_us":"#);
        assert!(line.starts_with(&prefix), "{line}");
        assert!(line.contains(r#","host_rx_unix_ns":"#), "{line}");
    }

    let hello = &lines[0];
    assert_eq!(hello["t_dev_us"], 1_500_000);
    assert_eq!(hello["fw_version"], "0.1.0");
    assert_eq!(hello["git_sha"], "abcdef12");
    assert_eq!(hello["device_mac"], "24:0a:c4:00:00:01");
    assert_eq!(hello["boot_channel"], 6);

    assert_eq!(lines[1]["level"], "info");
    assert_eq!(lines[1]["text"], "radio up ch=6");

    let st = &lines[2];
    for (k, v) in [
        ("uptime_s", 42),
        ("channel", 6),
        ("mode", 0),
        ("tracks_active", 1),
        ("heap_free", 40_000),
        ("mgmt_frames", 1000),
        ("beacons", 800),
        ("rid_frames", 12),
        ("beacon_errors", 0),
        ("pack_errors", 1),
        ("obs_dropped", 0),
        ("tx_dropped", 0),
        ("radio_errors", 0),
        ("log_dropped", 0),
    ] {
        assert_eq!(st[k], v, "{k}");
    }

    let obs = &lines[3];
    assert_eq!(obs["t_dev_us"], 42_123_456);
    assert_eq!(obs["channel"], 6);
    assert_eq!(obs["rssi_dbm"], -67);
    assert_eq!(obs["source"], "wifi_beacon");
    assert_eq!(obs["src_mac"], "02:11:22:33:44:55");
    assert_eq!(obs["msg_counter"], 42);
    assert_eq!(obs["t_mac_us"], 9_999_999);
    assert_eq!(obs["ie_len"], 133);
    assert_eq!(
        obs["pack_hex"],
        include_str!("../../../testdata/pack_5msg.hex").trim()
    );
    assert_eq!(obs["pack_error"], Value::Null);

    let m = obs["messages"].as_array().unwrap();
    let kinds: Vec<&str> = m.iter().map(|x| x["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        ["basic_id", "location", "self_id", "system", "operator_id"]
    );

    assert_eq!(m[0]["id_type"], 1);
    assert_eq!(m[0]["ua_type"], 2);
    assert_eq!(m[0]["uas_id"], "RIDPOCKET-TEST-0001");

    let loc = &m[1];
    assert_eq!(loc["status"], 2);
    assert_eq!(loc["height_type"], 0);
    assert_eq!(loc["track_deg"], 90);
    close(&loc["lat"], 45.0, 1e-7);
    close(&loc["lon"], -120.5, 1e-7);
    for (k, v) in [
        ("speed_h_mps", 5.0),
        ("speed_v_mps", 1.0),
        ("alt_baro_m", 145.5),
        ("alt_geo_m", 150.0),
        ("height_m", 50.0),
        ("timestamp_s", 1234.5),
        ("horiz_acc", 10.0),
        ("vert_acc", 4.0),
        ("baro_acc", 3.0),
        ("speed_acc", 3.0),
        ("ts_acc", 2.0),
    ] {
        close(&loc[k], v, 1e-3);
    }

    assert_eq!(m[2]["desc_type"], 0);
    assert_eq!(m[2]["desc"], "Bench test");

    let sys = &m[3];
    assert_eq!(sys["operator_location_type"], 1);
    assert_eq!(sys["classification_type"], 0);
    close(&sys["op_lat"], 45.25, 1e-7);
    close(&sys["op_lon"], -120.75, 1e-7);
    close(&sys["op_alt_geo_m"], 100.0, 1e-3);
    close(&sys["area_radius_m"], 0.0, 1e-3);
    assert_eq!(sys["area_count"], 1);
    assert_eq!(sys["area_ceiling_m"], Value::Null);
    assert_eq!(sys["area_floor_m"], Value::Null);
    assert_eq!(sys["timestamp_unix_s"], 1_790_208_000u64);

    assert_eq!(m[4]["operator_id_type"], 0);
    assert_eq!(m[4]["operator_id"], "OP-TEST-0001");
}
