use crate::mapping::Mappings;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    #[serde(default = "default_sampler_modes")]
    pub sampler_modes: Vec<crate::sampler::PlayMode>,
    pub input_version: u8,
    pub cue_seconds: f64,
    pub crossfade_seconds: f64,
    pub auto_dj_seconds: f64,
    pub auto_dj_source: String,
    pub library_monitor: bool,
    pub library_scan_minutes: u16,
    pub auto_dj_crossfade_seconds: f64,
    pub auto_dj_sample_count: u8,
    pub auto_dj_sample_spacing: f64,
    pub auto_dj_random_samples: bool,
    pub normalize_tracks: bool,
    pub effect_toggle: bool,
    pub sampler_volume: f64,
    pub audio_device: Option<String>,
    pub mic_device: Option<String>,
    pub mic_volume: f64,
    pub mic_mute: bool,
    pub mic_ducking: bool,
    pub linear_curve: bool,
    pub profiles: BTreeMap<String, Mappings>,
    pub active_profile: String,
}
fn default_sampler_modes() -> Vec<crate::sampler::PlayMode> {
    vec![crate::sampler::PlayMode::Restart; crate::sampler::SAMPLER_SLOTS]
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            sampler_modes: vec![crate::sampler::PlayMode::Restart; crate::sampler::SAMPLER_SLOTS],
            input_version: 0,
            cue_seconds: 3.,
            crossfade_seconds: 1.,
            auto_dj_seconds: 8.,
            auto_dj_source: "folder".into(),
            library_monitor: false,
            library_scan_minutes: 15,
            auto_dj_crossfade_seconds: 8.,
            auto_dj_sample_count: 1,
            auto_dj_sample_spacing: 2.,
            auto_dj_random_samples: false,
            normalize_tracks: false,
            effect_toggle: false,
            sampler_volume: 0.8,
            audio_device: None,
            mic_device: None,
            mic_volume: 0.8,
            mic_mute: false,
            mic_ducking: false,
            linear_curve: false,
            profiles: BTreeMap::new(),
            active_profile: "Default".into(),
        }
    }
}
impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        if !self.cue_seconds.is_finite() || !(0.1..=10.).contains(&self.cue_seconds) {
            return Err("Cue hold time must be between 0.1 and 10 seconds".into());
        }
        if !self.crossfade_seconds.is_finite() || !(0.1..=10.).contains(&self.crossfade_seconds) {
            return Err("Crossfade duration must be between 0.1 and 10 seconds".into());
        }
        if !self.auto_dj_seconds.is_finite() || !(1.0..=60.0).contains(&self.auto_dj_seconds) {
            return Err("Auto DJ timing must be between 1 and 60 seconds".into());
        }
        if !self.auto_dj_crossfade_seconds.is_finite()
            || !(0.1..=30.).contains(&self.auto_dj_crossfade_seconds)
        {
            return Err("Auto DJ crossfade must be 0.1–30 seconds".into());
        }
        if self.auto_dj_sample_count > 8 {
            return Err("Auto DJ sample count must be at most 8".into());
        }
        if !self.auto_dj_sample_spacing.is_finite()
            || !(0.1..=30.).contains(&self.auto_dj_sample_spacing)
        {
            return Err("Auto DJ sample spacing must be 0.1–30 seconds".into());
        }
        if !self.sampler_volume.is_finite() || !(0.0..=1.0).contains(&self.sampler_volume) {
            return Err("Sampler volume must be between 0 and 1".into());
        }
        if !self.mic_volume.is_finite() || !(0.0..=1.0).contains(&self.mic_volume) {
            return Err("Microphone volume must be between 0 and 1".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_migrate_and_short_cue_times_validate() {
        let mut p: Preferences = serde_json::from_str("{}").unwrap();
        assert_eq!(p.cue_seconds, 3.);
        assert!(!p.library_monitor);
        assert_eq!(p.library_scan_minutes, 15);
        p.cue_seconds = 0.3;
        p.validate().unwrap();
        p.crossfade_seconds = f64::NAN;
        assert!(p.validate().is_err());
    }
}
