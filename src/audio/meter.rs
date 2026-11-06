#[derive(Clone, Copy, Debug)]
pub struct MeterLevels {
    pub peak_l_db: f32,
    pub peak_r_db: f32,
    pub rms_l_db: f32,
    pub rms_r_db: f32,
    pub peak_hold_l_db: f32,
    pub peak_hold_r_db: f32,
    pub is_clipping: bool,
    pub clip_count: usize,
}

impl Default for MeterLevels {
    fn default() -> Self {
        Self {
            peak_l_db: -96.0,
            peak_r_db: -96.0,
            rms_l_db: -96.0,
            rms_r_db: -96.0,
            peak_hold_l_db: -96.0,
            peak_hold_r_db: -96.0,
            is_clipping: false,
            clip_count: 0,
        }
    }
}

pub fn linear_to_db(lin: f32) -> f32 {
    if lin <= 0.00001 {
        -96.0
    } else {
        (20.0 * lin.log10()).max(-96.0)
    }
}
