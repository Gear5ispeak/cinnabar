//! Gamepads on iOS, where gilrs has no backend: Apple's Game Controller framework supplies every
//! connected extended gamepad, such as a DualShock 4, DualSense or Xbox controller. Each frame
//! polls their elements and reports changes as Bevy's raw gamepad input, as gilrs does on other
//! platforms, so controller bindings, menus and gameplay read them unchanged.

use bevy::{
    input::{
        InputSystems,
        gamepad::{
            GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent,
            RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent,
        },
    },
    prelude::*,
};
use objc2::rc::Retained;
use objc2_game_controller::{GCController, GCControllerButtonInput, GCDevice, GCExtendedGamepad};

/// Buttons in the order [`read`] samples them, named as gilrs reports the same controls:
/// shoulders are the first triggers, analog triggers the second, Options/Menu is Start,
/// Share/View is Select and the PS/Xbox button is Mode.
const BUTTONS: [GamepadButton; 17] = [
    GamepadButton::South,
    GamepadButton::East,
    GamepadButton::West,
    GamepadButton::North,
    GamepadButton::LeftTrigger,
    GamepadButton::RightTrigger,
    GamepadButton::LeftTrigger2,
    GamepadButton::RightTrigger2,
    GamepadButton::Start,
    GamepadButton::Select,
    GamepadButton::Mode,
    GamepadButton::LeftThumb,
    GamepadButton::RightThumb,
    GamepadButton::DPadUp,
    GamepadButton::DPadDown,
    GamepadButton::DPadLeft,
    GamepadButton::DPadRight,
];

/// Stick axes in the order [`read`] samples them; up is positive, as gilrs reports it.
const AXES: [GamepadAxis; 4] = [
    GamepadAxis::LeftStickX,
    GamepadAxis::LeftStickY,
    GamepadAxis::RightStickX,
    GamepadAxis::RightStickY,
];

/// One controller's elements in [`BUTTONS`] and [`AXES`] order.
#[derive(Clone, Copy, Default, PartialEq)]
struct Sample {
    buttons: [f32; BUTTONS.len()],
    axes: [f32; AXES.len()],
}

/// A connected controller, the gamepad entity standing for it and its last reported sample.
struct Pad {
    controller: Retained<GCController>,
    entity: Entity,
    last: Sample,
}

/// Controllers the framework reported, kept by identity so a reordered list keeps entities.
#[derive(Default)]
struct Controllers(Vec<Pad>);

/// Polls controllers before Bevy processes the frame's raw gamepad input.
pub(crate) fn install(app: &mut App) {
    app.init_non_send::<Controllers>()
        .add_systems(PreUpdate, poll.before(InputSystems));
}

/// Reports connections, disconnections and every changed element as raw gamepad input.
fn poll(
    mut controllers: NonSendMut<Controllers>,
    mut commands: Commands,
    mut raw: MessageWriter<RawGamepadEvent>,
    mut connections: MessageWriter<GamepadConnectionEvent>,
    mut buttons: MessageWriter<RawGamepadButtonChangedEvent>,
    mut axes: MessageWriter<RawGamepadAxisChangedEvent>,
) {
    // SAFETY: the framework's controller list is read on the main thread, where this non-send
    // system runs.
    let listed = unsafe { GCController::controllers() };
    let connected: Vec<Retained<GCController>> = (0..listed.count())
        .map(|index| listed.objectAtIndex(index))
        .collect();
    controllers.0.retain(|pad| {
        let present = connected.contains(&pad.controller);
        if !present {
            let event = GamepadConnectionEvent::new(pad.entity, GamepadConnection::Disconnected);
            raw.write(event.clone().into());
            connections.write(event);
        }
        present
    });
    for controller in connected {
        if controllers.0.iter().any(|pad| pad.controller == controller) {
            continue;
        }
        // A Siri Remote or other micro gamepad has no sticks to play with.
        // SAFETY: as above; the profile is only queried.
        if unsafe { controller.extendedGamepad() }.is_none() {
            continue;
        }
        // SAFETY: as above.
        let name = unsafe { controller.vendorName() }
            .map_or_else(|| "Controller".to_owned(), |name| name.to_string());
        let entity = commands.spawn_empty().id();
        let event = GamepadConnectionEvent::new(
            entity,
            GamepadConnection::Connected {
                name,
                vendor_id: None,
                product_id: None,
            },
        );
        raw.write(event.clone().into());
        connections.write(event);
        controllers.0.push(Pad {
            controller,
            entity,
            last: Sample::default(),
        });
    }
    for pad in &mut controllers.0 {
        // SAFETY: as above.
        let Some(profile) = (unsafe { pad.controller.extendedGamepad() }) else {
            continue;
        };
        let sample = read(&profile);
        for (index, (value, last)) in sample.buttons.iter().zip(&pad.last.buttons).enumerate() {
            if value != last {
                let (button, value) = (BUTTONS[index], *value);
                raw.write(RawGamepadButtonChangedEvent::new(pad.entity, button, value).into());
                buttons.write(RawGamepadButtonChangedEvent::new(pad.entity, button, value));
            }
        }
        for (index, (value, last)) in sample.axes.iter().zip(&pad.last.axes).enumerate() {
            if value != last {
                let (axis, value) = (AXES[index], *value);
                raw.write(RawGamepadAxisChangedEvent::new(pad.entity, axis, value).into());
                axes.write(RawGamepadAxisChangedEvent::new(pad.entity, axis, value));
            }
        }
        pad.last = sample;
    }
}

/// Samples an extended gamepad's buttons, from 0 to 1, and stick axes, from -1 to 1.
fn read(profile: &GCExtendedGamepad) -> Sample {
    // SAFETY: element accessors only read the controller's current state, on the main thread.
    unsafe {
        let value = |button: Retained<GCControllerButtonInput>| button.value();
        let optional =
            |button: Option<Retained<GCControllerButtonInput>>| button.map_or(0.0, |b| b.value());
        let dpad = profile.dpad();
        let (left, right) = (profile.leftThumbstick(), profile.rightThumbstick());
        Sample {
            buttons: [
                value(profile.buttonA()),
                value(profile.buttonB()),
                value(profile.buttonX()),
                value(profile.buttonY()),
                value(profile.leftShoulder()),
                value(profile.rightShoulder()),
                value(profile.leftTrigger()),
                value(profile.rightTrigger()),
                value(profile.buttonMenu()),
                optional(profile.buttonOptions()),
                optional(profile.buttonHome()),
                optional(profile.leftThumbstickButton()),
                optional(profile.rightThumbstickButton()),
                value(dpad.up()),
                value(dpad.down()),
                value(dpad.left()),
                value(dpad.right()),
            ],
            axes: [
                left.xAxis().value(),
                left.yAxis().value(),
                right.xAxis().value(),
                right.yAxis().value(),
            ],
        }
    }
}
