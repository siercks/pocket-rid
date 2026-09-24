use crate::{AuthPage, BasicId, Location, ODID_EPOCH_UNIX_S, OperatorId, SelfId, System};

/// Raw 0 (−1000 m) is the invalid marker.
fn alt(v: u16) -> Option<f32> {
    (v != 0).then(|| f32::from(v) * 0.5 - 1000.0)
}

fn position(lat_e7: i32, lon_e7: i32) -> Option<(f64, f64)> {
    let invalid = (lat_e7 == 0 && lon_e7 == 0)
        || lat_e7.unsigned_abs() > 900_000_000
        || lon_e7.unsigned_abs() > 1_800_000_000;
    (!invalid).then(|| (f64::from(lat_e7) * 1e-7, f64::from(lon_e7) * 1e-7))
}

/// Bytes before the first NUL; invalid UTF-8 is cut at the last valid char.
fn text(b: &[u8]) -> &str {
    let b = b.split(|&c| c == 0).next().unwrap_or_default();
    match core::str::from_utf8(b) {
        Ok(s) => s,
        Err(e) => b
            .get(..e.valid_up_to())
            .and_then(|v| core::str::from_utf8(v).ok())
            .unwrap_or_default(),
    }
}

impl BasicId {
    pub fn as_str(&self) -> &str {
        text(&self.uas_id)
    }
}

impl SelfId {
    pub fn as_str(&self) -> &str {
        text(&self.desc)
    }
}

impl OperatorId {
    pub fn as_str(&self) -> &str {
        text(&self.operator_id)
    }
}

impl Location {
    pub fn track_deg(&self) -> Option<u16> {
        let t = u16::from(self.direction_raw).saturating_add(if self.ew_direction != 0 {
            180
        } else {
            0
        });
        (t <= 360).then_some(t)
    }
    pub fn speed_h_mps(&self) -> Option<f32> {
        let raw = f32::from(self.speed_h_raw);
        match (self.speed_mult, self.speed_h_raw) {
            (0, _) => Some(raw * 0.25),
            (_, 255) => None,
            _ => Some(raw * 0.75 + 63.75),
        }
    }
    pub fn speed_v_mps(&self) -> Option<f32> {
        (self.speed_v_raw != 126).then(|| f32::from(self.speed_v_raw) * 0.5)
    }
    pub fn latitude_deg(&self) -> Option<f64> {
        position(self.lat_e7, self.lon_e7).map(|p| p.0)
    }
    pub fn longitude_deg(&self) -> Option<f64> {
        position(self.lat_e7, self.lon_e7).map(|p| p.1)
    }
    pub fn alt_baro_m(&self) -> Option<f32> {
        alt(self.alt_baro_raw)
    }
    pub fn alt_geo_m(&self) -> Option<f32> {
        alt(self.alt_geo_raw)
    }
    pub fn height_m(&self) -> Option<f32> {
        alt(self.height_raw)
    }
    pub fn timestamp_s(&self) -> Option<f32> {
        (self.timestamp_raw <= 36000).then(|| f32::from(self.timestamp_raw) * 0.1)
    }
}

impl System {
    pub fn latitude_deg(&self) -> Option<f64> {
        position(self.op_lat_e7, self.op_lon_e7).map(|p| p.0)
    }
    pub fn longitude_deg(&self) -> Option<f64> {
        position(self.op_lat_e7, self.op_lon_e7).map(|p| p.1)
    }
    pub fn area_radius_m(&self) -> f32 {
        f32::from(self.area_radius_raw) * 10.0
    }
    pub fn area_ceiling_m(&self) -> Option<f32> {
        alt(self.area_ceiling_raw)
    }
    pub fn area_floor_m(&self) -> Option<f32> {
        alt(self.area_floor_raw)
    }
    pub fn op_alt_geo_m(&self) -> Option<f32> {
        alt(self.op_alt_geo_raw)
    }
    pub fn timestamp_unix_s(&self) -> u64 {
        u64::from(self.timestamp).saturating_add(ODID_EPOCH_UNIX_S)
    }
}

impl AuthPage {
    /// `None` unless this is page 0; later pages carry only signature bytes.
    fn page0(&self) -> Option<&[u8; 23]> {
        (self.data_page == 0).then_some(&self.data)
    }
    pub fn last_page_index(&self) -> Option<u8> {
        self.page0().map(|d| d[0])
    }
    pub fn length(&self) -> Option<u8> {
        self.page0().map(|d| d[1])
    }
    pub fn timestamp_unix_s(&self) -> Option<u64> {
        self.page0().map(|d| {
            u64::from(u32::from_le_bytes([d[2], d[3], d[4], d[5]]))
                .saturating_add(ODID_EPOCH_UNIX_S)
        })
    }
}
