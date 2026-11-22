//! Persistent insert order. The main mix is always outside this list.
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectStage {
    Channels,
    Restoration,
    Equalizer,
    Expander,
    Compressor,
    Limiter,
}

impl EffectStage {
    pub const ALL: [Self; 6] = [
        Self::Channels,
        Self::Restoration,
        Self::Equalizer,
        Self::Expander,
        Self::Compressor,
        Self::Limiter,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitorStage {
    Insert(EffectStage),
    Mix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectSlot {
    pub stage: EffectStage,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

fn enabled() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EffectChain([EffectSlot; EffectStage::ALL.len()]);

impl Default for EffectChain {
    fn default() -> Self {
        Self(EffectStage::ALL.map(|stage| EffectSlot {
            stage,
            enabled: true,
        }))
    }
}

impl<'de> Deserialize<'de> for EffectChain {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = Vec::<EffectSlot>::deserialize(deserializer)?;
        let mut chain = Self::default();
        let mut count = 0;
        for slot in saved.into_iter().chain(Self::default().0) {
            if !chain.0[..count]
                .iter()
                .any(|known| known.stage == slot.stage)
            {
                chain.0[count] = slot;
                count += 1;
            }
        }
        Ok(chain)
    }
}

impl EffectChain {
    pub fn slots(&self) -> &[EffectSlot] {
        &self.0
    }

    pub fn position(&self, stage: EffectStage) -> usize {
        self.0
            .iter()
            .position(|slot| slot.stage == stage)
            .expect("every built-in insert has a slot")
    }

    pub fn enabled(&self, stage: EffectStage) -> bool {
        self.0[self.position(stage)].enabled
    }

    pub fn set_enabled(&mut self, stage: EffectStage, enabled: bool) {
        let index = self.position(stage);
        self.0[index].enabled = enabled;
    }

    pub fn move_by(&mut self, stage: EffectStage, offset: isize) -> bool {
        let from = self.position(stage);
        let Some(to) = from
            .checked_add_signed(offset)
            .filter(|&to| to < self.0.len())
        else {
            return false;
        };
        if from < to {
            self.0[from..=to].rotate_left(1);
        } else {
            self.0[to..=from].rotate_right(1);
        }
        from != to
    }

    pub(super) fn same_order(&self, other: &Self) -> bool {
        self.0
            .iter()
            .zip(other.0)
            .all(|(left, right)| left.stage == right.stage)
    }
}
