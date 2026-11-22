use super::player::DecodedAudio;
use super::recorder::StereoWaveformPoint;

#[derive(Default)]
pub(crate) struct WaveformCache {
    // Finest to coarsest, with uniform time bins.
    levels: Vec<Vec<StereoWaveformPoint>>,
}

impl WaveformCache {
    pub(crate) fn build(audio: &DecodedAudio) -> Self {
        let frames = audio.left.len();
        if frames == 0 {
            return Self::default();
        }
        let bins = frames.min(65_536);
        let mut finest = Vec::with_capacity(bins);
        for index in 0..bins {
            let start = index * frames / bins;
            let end = (index + 1) * frames / bins;
            let mut point = StereoWaveformPoint::default();
            for (&l, &r) in audio.left[start..end].iter().zip(&audio.right[start..end]) {
                point.min_l = point.min_l.min(l);
                point.max_l = point.max_l.max(l);
                point.min_r = point.min_r.min(r);
                point.max_r = point.max_r.max(r);
            }
            finest.push(point);
        }
        let mut levels = vec![finest];
        while levels
            .last()
            .is_some_and(|level| level.len() > 4096 && level.len() % 2 == 0)
        {
            let next = levels
                .last()
                .unwrap()
                .chunks_exact(2)
                .map(|pair| StereoWaveformPoint {
                    min_l: pair[0].min_l.min(pair[1].min_l),
                    max_l: pair[0].max_l.max(pair[1].max_l),
                    min_r: pair[0].min_r.min(pair[1].min_r),
                    max_r: pair[0].max_r.max(pair[1].max_r),
                })
                .collect();
            levels.push(next);
        }
        Self { levels }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }

    pub(crate) fn for_view(&self, zoom: f64) -> &[StereoWaveformPoint] {
        self.levels
            .iter()
            .rev()
            .find(|level| level.len() as f64 >= 2000.0 * zoom)
            .or_else(|| self.levels.first())
            .map_or(&[], Vec::as_slice)
    }
}
