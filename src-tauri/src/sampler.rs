use serde::{Deserialize, Serialize};

pub const SAMPLER_SLOTS: usize = 48;

/// Behavior belongs to the pad, independently of the input mapped to trigger it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    #[default]
    Restart,
    Toggle,
    Overlap,
    Hold,
}
