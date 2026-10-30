//! Saved touch-control positions, expressed as fractions of available safe-screen travel.
use super::SettingsOptions;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Fixed-point resolution of persisted normalized positions.
const UNITS: f32 = 10_000.;
/// Mod button placements kept for mods that are not loaded, beyond which later names drop.
const MAX_MOD_PLACEMENTS: usize = 64;

/// Only explicit placements are stored; an empty map retains the authored touch layout.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TouchLayout(BTreeMap<String, [u16; 2]>);

impl TouchLayout {
    /// Reads a validated position as a fraction of travel after subtracting control size.
    pub fn position(&self, name: &str) -> Option<[f32; 2]> {
        self.0
            .get(name)
            .map(|position| position.map(|v| f32::from(v) / UNITS))
    }
    /// Stores a finite, bounded position. Control identities are checked when loading settings.
    pub fn set(&mut self, name: &str, position: [f32; 2]) {
        if position
            .into_iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
        {
            self.0.insert(
                name.to_owned(),
                position.map(|v| (v * UNITS).round() as u16),
            );
        }
    }
    /// Keeps only the placements whose names satisfy `keep`.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.0.retain(|name, _| keep(name));
    }
    /// Removes obsolete control identities and malformed fixed-point coordinates. Mod button
    /// placements are kept by name, bounded, whether or not their mod is loaded.
    pub(super) fn validate(&mut self) {
        if let Some(position) = self.0.remove("emote") {
            self.0.entry("utilities".into()).or_insert(position);
        }
        let document: serde_json::Value =
            serde_json::from_slice(assets::TOUCH_LAYOUT).expect("built-in touch layout parses");
        let mut names = std::collections::BTreeSet::new();
        control_names(&document, &mut names);
        let mut mods = 0;
        self.0.retain(|name, at| {
            let known = if ui::mod_touch::is_placement_name(name) {
                mods += 1;
                mods <= MAX_MOD_PLACEMENTS
            } else {
                name != "look" && names.contains(name.as_str())
            };
            known && at.iter().all(|v| f32::from(*v) <= UNITS)
        });
    }
}
/// Derives identities from the authored control actions rather than maintaining a second registry.
fn control_names<'a>(
    value: &'a serde_json::Value,
    names: &mut std::collections::BTreeSet<&'a str>,
) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(name) = text
                .strip_prefix("cinnabar.touch.")
                .filter(|name| !name.is_empty())
            {
                names.insert(name);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                control_names(value, names);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                control_names(value, names);
            }
        }
        _ => {}
    }
}

impl SettingsOptions {
    /// The saved layout used by both touch rendering and input regions.
    pub fn touch_layout(&self) -> &TouchLayout {
        &self.touch_layout
    }
    /// The chosen touch control scheme.
    pub fn touch_scheme(&self) -> semantic_input::TouchScheme {
        semantic_input::TouchScheme::from_index(
            usize::try_from(self.value(super::SettingId::TouchControlScheme)).unwrap_or_default(),
        )
    }
    /// How much larger than the authored layout the movement and action controls draw, where
    /// the slider's default is the authored size.
    pub fn touch_button_scale(&self) -> f32 {
        let id = super::SettingId::TouchButtonSize;
        self.value(id) as f32 / id.definition().default as f32
    }

    /// Whether the classic D-pad sprints while it moves forward.
    pub fn touch_classic_auto_sprint(&self) -> bool {
        self.value(super::SettingId::TouchClassicAutoSprint) != 0
    }
    /// Replaces a completed layout draft; callers persist only after Save.
    pub fn set_touch_layout(&mut self, mut layout: TouchLayout) {
        layout.validate();
        self.touch_layout = layout;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_touch_scheme_defaults_to_modern_split_and_persists() {
        let mut options = SettingsOptions::default();
        assert_eq!(options.touch_scheme(), semantic_input::TouchScheme::default());
        let classic_tap = semantic_input::TouchScheme::ALL[0];
        assert!(options.set(
            super::super::SettingId::TouchControlScheme.index(),
            classic_tap.index() as i32
        ));
        let decoded = SettingsOptions::decode(&serde_json::to_vec(&options).unwrap()).unwrap();
        assert_eq!(decoded.touch_scheme(), classic_tap);
    }

    #[test]
    fn touch_buttons_default_to_the_authored_size_and_can_double() {
        let mut options = SettingsOptions::default();
        assert_eq!(options.touch_button_scale(), 1.0);
        assert!(options.set(super::super::SettingId::TouchButtonSize.index(), 100));
        assert_eq!(options.touch_button_scale(), 2.0);
        assert!(options.set(super::super::SettingId::TouchButtonSize.index(), 25));
        assert_eq!(options.touch_button_scale(), 0.5);
    }

    #[test]
    fn classic_auto_sprint_defaults_on_and_turns_off() {
        let mut options = SettingsOptions::default();
        assert!(options.touch_classic_auto_sprint());
        assert!(options.set(
            super::super::SettingId::TouchClassicAutoSprint.index(),
            0
        ));
        let decoded = SettingsOptions::decode(&serde_json::to_vec(&options).unwrap()).unwrap();
        assert!(!decoded.touch_classic_auto_sprint());
    }

    #[test]
    fn emote_button_placement_migrates_to_the_utility_trigger() {
        let options =
            SettingsOptions::decode(br#"{"touch_layout":{"emote":[1234,7500]}}"#).unwrap();
        assert_eq!(
            options.touch_layout().position("utilities"),
            Some([0.1234, 0.75])
        );
        assert_eq!(options.touch_layout().position("emote"), None);
        let both = SettingsOptions::decode(
            br#"{"touch_layout":{"emote":[1234,7500],"utilities":[2500,5000]}}"#,
        )
        .unwrap();
        assert_eq!(both.touch_layout().position("utilities"), Some([0.25, 0.5]));
    }

    #[test]
    fn positions_round_trip_and_reject_stale_or_invalid_controls() {
        let mut options = SettingsOptions::default();
        let mut layout = TouchLayout::default();
        layout.set("jump", [0.1234, 0.75]);
        layout.set("look", [0.5, 0.5]);
        layout.set("old-control", [0.5, 0.5]);
        layout.set("chat", [f32::NAN, 0.]);
        options.set_touch_layout(layout);
        let decoded = SettingsOptions::decode(&serde_json::to_vec(&options).unwrap()).unwrap();
        assert_eq!(
            decoded.touch_layout().position("jump"),
            Some([0.1234, 0.75])
        );
        for name in ["look", "old-control", "chat"] {
            assert_eq!(decoded.touch_layout().position(name), None);
        }
        let old = SettingsOptions::decode(b"{}").unwrap();
        assert_eq!(old.touch_layout(), &TouchLayout::default());
        let corrupt = SettingsOptions::decode(br#"{"touch_layout":{"jump":[65535,4]}}"#).unwrap();
        assert_eq!(corrupt.touch_layout().position("jump"), None);
    }

    /// Mod button placements persist by id while their mod is unloaded, within a bound.
    #[test]
    fn mod_button_placements_persist_by_name_and_are_bounded() {
        let mut layout = TouchLayout::default();
        layout.set("mod-client-menu", [0.25, 0.5]);
        layout.set("mod-Bad", [0.25, 0.5]);
        for index in 0..MAX_MOD_PLACEMENTS {
            layout.set(&format!("mod-x{index:03}"), [0.5, 0.5]);
        }
        let mut options = SettingsOptions::default();
        options.set_touch_layout(layout);
        let decoded = SettingsOptions::decode(&serde_json::to_vec(&options).unwrap()).unwrap();
        let layout = decoded.touch_layout();
        assert_eq!(layout.position("mod-client-menu"), Some([0.25, 0.5]));
        assert_eq!(layout.position("mod-Bad"), None);
        assert_eq!(layout.position("mod-x000"), Some([0.5, 0.5]));
        assert_eq!(
            layout.position(&format!("mod-x{:03}", MAX_MOD_PLACEMENTS - 1)),
            None
        );
    }
}
