use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const BUTTONS: &[&str] = &[
    "a",
    "b",
    "x",
    "y",
    "lb",
    "rb",
    "lt",
    "rt",
    "l3",
    "r3",
    "menu",
    "view",
    "dpad-up",
    "dpad-down",
    "dpad-left",
    "dpad-right",
];
pub const AXES: &[&str] = &["left-x", "left-y", "right-x", "right-y"];
pub const ACTIONS: &[&str] = &[
    "none",
    "cue_hold",
    "play",
    "set_cue",
    "load_selected",
    "load_next",
    "load_1",
    "load_2",
    "queue_add",
    "browse_up",
    "browse_down",
    "panel",
    "backspin",
    "brake",
    "start",
    "echo",
    "reverb",
    "sample",
    "next_sample",
    "stop_samples",
    "target_next",
    "auto_target",
    "shift",
    "tempo_match",
    "auto_cross",
];
pub const AXIS_ACTIONS: &[&str] = &["none", "cross", "filter", "wet", "volume", "master"];
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mappings {
    pub normal: BTreeMap<String, String>,
    pub shift: BTreeMap<String, String>,
}
impl Default for Mappings {
    fn default() -> Self {
        let mut normal = BTreeMap::new();
        for c in BUTTONS.iter().chain(AXES) {
            normal.insert(c.to_string(), "none".into());
        }
        for (c, a) in [
            ("a", "load_selected"),
            ("b", "cue_hold"),
            ("x", "load_next"),
            ("y", "queue_add"),
            ("lb", "backspin"),
            ("rb", "brake"),
            ("r3", "sample"),
            ("l3", "auto_cross"),
            ("menu", "target_next"),
            ("view", "shift"),
            ("dpad-up", "browse_up"),
            ("dpad-down", "browse_down"),
            ("dpad-left", "panel"),
            ("dpad-right", "panel"),
            ("left-x", "cross"),
            ("right-x", "filter"),
            ("right-y", "wet"),
        ] {
            normal.insert(c.into(), a.into());
        }
        let mut shift = normal.clone();
        for (c, a) in [
            ("a", "load_1"),
            ("b", "load_2"),
            ("x", "tempo_match"),
            ("y", "none"),
            ("lb", "echo"),
            ("rb", "reverb"),
            ("r3", "next_sample"),
        ] {
            shift.insert(c.into(), a.into());
        }
        Self { normal, shift }
    }
}
impl Mappings {
    pub fn validate(&self) -> Result<(), String> {
        for map in [&self.normal, &self.shift] {
            if map.len() != BUTTONS.len() + AXES.len() {
                return Err("Each layer must contain every controller input exactly once".into());
            }
            for (c, a) in map {
                let allowed = if AXES.contains(&c.as_str()) {
                    AXIS_ACTIONS
                } else if BUTTONS.contains(&c.as_str()) {
                    ACTIONS
                } else {
                    return Err(format!("Unknown controller input: {c}"));
                };
                if !allowed.contains(&a.as_str()) {
                    return Err(format!("Unsupported mapping {c}: {a}"));
                }
            }
        }
        Ok(())
    }
    pub fn get(&self, shift: bool, control: &str) -> String {
        (if shift { &self.shift } else { &self.normal })
            .get(control)
            .cloned()
            .unwrap_or_else(|| "none".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_roundtrip() {
        let m = Mappings::default();
        assert_eq!(m.get(false, "b"), "cue_hold");
        assert_eq!(m.get(true, "b"), "load_2");
        m.validate().unwrap();
        let round: Mappings = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(round.normal, m.normal);
    }
    #[test]
    fn invalid_actions_rejected() {
        let mut m = Mappings::default();
        m.normal.insert("b".into(), "execute_shell".into());
        assert!(m.validate().is_err());
        m.normal.insert("b".into(), "cross".into());
        assert!(m.validate().is_err());
    }
}
