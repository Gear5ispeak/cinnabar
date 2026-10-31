//! Device-specific bindings share the physical control codes used by gameplay.

#[cfg(test)]
use super::BindingId;
use super::SettingId;
use super::bindings::EXTRA_KEYS;
#[cfg(test)]
use super::bindings::{EXTRA_GAMEPAD, GAMEPAD_BINDINGS};
use semantic_input::{AxisDirection, PhysicalControl};

/// Persisted empty assignment; an absent entry instead selects the default binding.
pub const UNBOUND_CONTROL: u16 = 0;

pub const GAMEPAD_OFFSET: usize = 0x8000;
/// Keeps keyboard and gamepad bindings in disjoint persisted code ranges.
pub fn encode_control(control: PhysicalControl) -> Option<u16> {
    match control {
        PhysicalControl::KeyboardUsage(code) if (0x04..=0xe7).contains(&code) => Some(code),
        PhysicalControl::MouseButton(button) if (1..=8).contains(&button) => {
            Some(0x100 + u16::from(button))
        }
        PhysicalControl::GamepadButton(button) if button <= 31 => Some(0x200 + u16::from(button)),
        PhysicalControl::GamepadAxis {
            axis: axis @ 4..=5,
            direction: AxisDirection::Positive,
        } => Some(0x300 + u16::from(axis)),
        _ => None,
    }
}

/// Rejects persisted controls the desktop capture path cannot produce.
pub fn decode_control(code: u16) -> Option<PhysicalControl> {
    match code {
        0x04..=0xe7 => Some(PhysicalControl::KeyboardUsage(code)),
        0x101..=0x108 => Some(PhysicalControl::MouseButton((code - 0x100) as u8)),
        0x200..=0x21f => Some(PhysicalControl::GamepadButton((code - 0x200) as u8)),
        0x304..=0x305 => Some(PhysicalControl::GamepadAxis {
            axis: (code - 0x300) as u8,
            direction: AxisDirection::Positive,
        }),
        _ => None,
    }
}

/// Identifies gamepad controls without treating the movement sticks as remappable buttons.
pub fn is_gamepad(control: PhysicalControl) -> bool {
    matches!(
        control,
        PhysicalControl::GamepadButton(_) | PhysicalControl::GamepadAxis { .. }
    )
}

/// Controller button art the player chose; each family has a full glyph set in the vanilla pack.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(i32)]
pub enum ControllerIcons {
    #[default]
    Xbox,
    Ps4,
    Ps5,
}

impl ControllerIcons {
    /// Every family in its persisted order; index `i` is saved as value `i`.
    pub const ALL: [Self; 3] = [Self::Xbox, Self::Ps4, Self::Ps5];

    /// The vanilla pack's texture prefix for this family, as in `textures/ui/ps5_bumper_left`.
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Xbox => "xbox",
            Self::Ps4 => "ps4",
            Self::Ps5 => "ps5",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Xbox => "Xbox",
            Self::Ps4 => "PlayStation 4",
            Self::Ps5 => "PlayStation 5",
        }
    }

    const fn choice_name(self) -> &'static str {
        match self {
            Self::Xbox => "controller_icons_radio_xbox",
            Self::Ps4 => "controller_icons_radio_ps4",
            Self::Ps5 => "controller_icons_radio_ps5",
        }
    }

    /// Reads a saved value, falling back to Xbox for anything out of range.
    pub fn from_value(value: i32) -> Self {
        usize::try_from(value)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
            .unwrap_or_default()
    }

    /// The control's name in this family's printed labels, such as `Cross` or `L1`.
    pub fn button_name(self, control: PhysicalControl) -> Option<&'static str> {
        let playstation = self != Self::Xbox;
        Some(match control {
            PhysicalControl::GamepadButton(0) if playstation => "Cross",
            PhysicalControl::GamepadButton(1) if playstation => "Circle",
            PhysicalControl::GamepadButton(2) if playstation => "Triangle",
            PhysicalControl::GamepadButton(3) if playstation => "Square",
            PhysicalControl::GamepadButton(4) if playstation => "L1",
            PhysicalControl::GamepadButton(5) if playstation => "R1",
            PhysicalControl::GamepadButton(6) if self == Self::Ps4 => "Share",
            PhysicalControl::GamepadButton(6) if self == Self::Ps5 => "Create",
            PhysicalControl::GamepadButton(7) if playstation => "Options",
            PhysicalControl::GamepadButton(8) if playstation => "L3",
            PhysicalControl::GamepadButton(9) if playstation => "R3",
            PhysicalControl::GamepadAxis { axis: 4, .. } if playstation => "L2",
            PhysicalControl::GamepadAxis { axis: 5, .. } if playstation => "R2",
            PhysicalControl::GamepadButton(6) => "View",
            PhysicalControl::GamepadButton(7) => "Menu",
            _ => return None,
        })
    }
}

/// Dropdown choices derived from [`ControllerIcons::ALL`]; each radio index is the saved value.
pub const CONTROLLER_ICON_CHOICES: &[super::definitions::SettingChoice] = &{
    let all = ControllerIcons::ALL;
    let mut choices = [super::definitions::SettingChoice {
        name: "",
        label: "",
    }; ControllerIcons::ALL.len()];
    let mut index = 0;
    while index < choices.len() {
        assert!(all[index] as usize == index);
        choices[index] = super::definitions::SettingChoice {
            name: all[index].choice_name(),
            label: all[index].label(),
        };
        index += 1;
    }
    choices
};

/// The glyph name shared by every family's texture, without its family prefix.
const fn icon_suffix(control: PhysicalControl) -> Option<&'static str> {
    Some(match control {
        PhysicalControl::GamepadButton(0) => "face_button_down",
        PhysicalControl::GamepadButton(1) => "face_button_right",
        PhysicalControl::GamepadButton(2) => "face_button_up",
        PhysicalControl::GamepadButton(3) => "face_button_left",
        PhysicalControl::GamepadButton(4) => "bumper_left",
        PhysicalControl::GamepadButton(5) => "bumper_right",
        PhysicalControl::GamepadButton(6) => "select_button",
        PhysicalControl::GamepadButton(7) => "start_button",
        PhysicalControl::GamepadButton(8) => "stick_left",
        PhysicalControl::GamepadButton(9) => "stick_right",
        PhysicalControl::GamepadButton(11) => "dpad_up",
        PhysicalControl::GamepadButton(12) => "dpad_down",
        PhysicalControl::GamepadButton(13) => "dpad_left",
        PhysicalControl::GamepadButton(14) => "dpad_right",
        PhysicalControl::GamepadAxis { axis: 4, .. } => "left_trigger",
        PhysicalControl::GamepadAxis { axis: 5, .. } => "right_trigger",
        _ => return None,
    })
}

/// Displays the pack's glyph for a captured gamepad control in the chosen family, or an empty
/// path for controls without one.
pub fn gamepad_icon(control: PhysicalControl, icons: ControllerIcons) -> String {
    icon_suffix(control)
        .map(|suffix| format!("textures/ui/{}_{suffix}", icons.prefix()))
        .unwrap_or_default()
}

impl super::SettingsOptions {
    /// Imports a keyboard layout as one validated batch, so swapped keys do not conflict midway.
    pub fn import_keyboard_bindings(
        &mut self,
        bindings: &std::collections::BTreeMap<String, Option<PhysicalControl>>,
    ) -> usize {
        let mut candidate = self.clone();
        let mut accepted = 0;
        for (name, control) in bindings {
            if !super::KEY_BINDINGS.iter().any(|(_, key)| *key == name)
                && !EXTRA_KEYS.iter().any(|(key, _)| *key == name)
            {
                continue;
            }
            let code = match control {
                None => UNBOUND_CONTROL,
                Some(control) if !is_gamepad(*control) => match encode_control(*control) {
                    Some(code) => code,
                    None => continue,
                },
                _ => continue,
            };
            candidate.keys.insert(name.clone(), code);
            accepted += 1;
        }
        if !candidate.stored_bindings_valid() {
            return bindings.len();
        }
        *self = candidate;
        bindings.len() - accepted
    }

    /// The button art family chosen in Controller settings.
    pub fn controller_icons(&self) -> ControllerIcons {
        ControllerIcons::from_value(self.value(SettingId::ControllerIcons))
    }

    /// Applies the vanilla A/B and X/Y physical-button swaps after resolving a binding.
    pub fn swap_gamepad_control(&self, control: PhysicalControl) -> PhysicalControl {
        let PhysicalControl::GamepadButton(button) = control else {
            return control;
        };
        let button = match button {
            0 | 1 if self.value(SettingId::SwapGamepadAbButtons) != 0 => 1 - button,
            2 | 3 if self.value(SettingId::SwapGamepadXyButtons) != 0 => 5 - button,
            _ => button,
        };
        PhysicalControl::GamepadButton(button)
    }
}

#[cfg(test)]
mod emote_tests {
    use super::*;

    #[test]
    fn legacy_supplemental_rows_keep_saved_drop_and_screenshot_bindings() {
        let loaded = super::super::SettingsOptions::decode(
            br#"{"keys":{"key.drop":69,"key.screenshot":70}}"#,
        )
        .unwrap();
        let base = super::super::KEY_BINDINGS.len();
        assert_eq!(
            loaded.key_control(base + 3),
            Some(PhysicalControl::KeyboardUsage(69))
        );
        assert_eq!(
            loaded.key_control(base + 5),
            Some(PhysicalControl::KeyboardUsage(70))
        );
        assert_eq!(
            loaded.named_key_control(BindingId::Emote),
            Some(PhysicalControl::KeyboardUsage(0x05))
        );
    }

    #[test]
    fn native_emote_gamepad_default_and_device_remapping_survive_reload() {
        let row = EXTRA_GAMEPAD
            .iter()
            .position(|(name, _)| *name == "key.emote")
            .unwrap();
        let index = GAMEPAD_OFFSET + GAMEPAD_BINDINGS.len() + row;
        let mut settings = super::super::SettingsOptions::default();
        assert_eq!(
            settings.key_control(index),
            Some(PhysicalControl::GamepadButton(13))
        );
        assert_eq!(
            gamepad_icon(
                settings.key_control(index).unwrap(),
                settings.controller_icons()
            ),
            "textures/ui/xbox_dpad_left"
        );
        assert!(settings.remap(index, PhysicalControl::GamepadButton(9)));
        let mut loaded =
            super::super::SettingsOptions::decode(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(
            loaded.key_control(index),
            Some(PhysicalControl::GamepadButton(9))
        );
        assert_eq!(
            loaded.named_key_control(BindingId::Emote),
            Some(PhysicalControl::KeyboardUsage(0x05))
        );
        assert!(loaded.reset_key(index));
        assert_eq!(
            loaded.key_control(index),
            Some(PhysicalControl::GamepadButton(13))
        );
    }
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    #[test]
    fn playstation_icons_replace_xbox_glyphs_until_switched_back() {
        let mut settings = super::super::SettingsOptions::default();
        let cross = PhysicalControl::GamepadButton(0);
        let l2 = PhysicalControl::GamepadAxis {
            axis: 4,
            direction: AxisDirection::Positive,
        };
        assert_eq!(settings.controller_icons(), ControllerIcons::Xbox);
        assert_eq!(
            gamepad_icon(cross, settings.controller_icons()),
            "textures/ui/xbox_face_button_down"
        );
        assert_eq!(settings.key_name(cross), "A");

        settings.set(
            SettingId::ControllerIcons.index(),
            ControllerIcons::Ps5 as i32,
        );
        assert_eq!(
            gamepad_icon(cross, settings.controller_icons()),
            "textures/ui/ps5_face_button_down"
        );
        assert_eq!(
            gamepad_icon(l2, settings.controller_icons()),
            "textures/ui/ps5_left_trigger"
        );
        assert_eq!(settings.key_name(cross), "Cross");
        assert_eq!(settings.key_name(l2), "L2");
        assert_eq!(
            settings.key_name(PhysicalControl::GamepadButton(6)),
            "Create"
        );

        settings.set(
            SettingId::ControllerIcons.index(),
            ControllerIcons::Ps4 as i32,
        );
        assert_eq!(
            gamepad_icon(
                PhysicalControl::GamepadButton(4),
                settings.controller_icons()
            ),
            "textures/ui/ps4_bumper_left"
        );
        assert_eq!(
            settings.key_name(PhysicalControl::GamepadButton(6)),
            "Share"
        );

        settings.set(
            SettingId::ControllerIcons.index(),
            ControllerIcons::Xbox as i32,
        );
        assert_eq!(
            gamepad_icon(cross, settings.controller_icons()),
            "textures/ui/xbox_face_button_down"
        );
        assert_eq!(settings.key_name(cross), "A");
    }

    #[test]
    fn chosen_icon_family_survives_reload() {
        let mut settings = super::super::SettingsOptions::default();
        settings.set(
            SettingId::ControllerIcons.index(),
            ControllerIcons::Ps4 as i32,
        );
        let loaded =
            super::super::SettingsOptions::decode(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(loaded.controller_icons(), ControllerIcons::Ps4);
    }

    #[test]
    fn out_of_range_values_fall_back_to_xbox() {
        assert_eq!(ControllerIcons::from_value(-1), ControllerIcons::Xbox);
        assert_eq!(ControllerIcons::from_value(99), ControllerIcons::Xbox);
    }
}
