//! Look-surface taps and holds for the touch schemes that act through the screen rather than
//! dedicated buttons: a tap uses, or attacks the actor the ray is on, and a hold breaks.

use bevy::{
    ecs::system::SystemParam,
    prelude::{Local, Res, ResMut},
    time::{Real, Time},
};
use semantic_input::{
    TouchAim, TouchContact, TouchGestures, TouchIndicator, TouchMovement, TouchScheme, touch,
};

use crate::{camera::touch_aim::TouchAimPoint, semantic_controls::SemanticTouchTargets};

/// Synthetic action contacts carry their finger's id with the top bit set, so they never
/// collide with platform ids.
const ACTION_CONTACT: u64 = 1 << 63;
/// Frames a lifted tap keeps aiming at its point: its own and the next, since actions resolve
/// against the previous published frame.
const TAP_AIM_FRAMES: u8 = 2;

#[derive(Default)]
pub(crate) struct GestureState {
    gestures: TouchGestures,
    /// The last tap's point and the frames it still aims for.
    tap: Option<([f32; 2], u8)>,
}

#[derive(SystemParam)]
pub(crate) struct LookGestures<'w, 's> {
    state: Local<'s, GestureState>,
    time: Option<Res<'w, Time<Real>>>,
    aim: Option<ResMut<'w, TouchAimPoint>>,
    movement: Option<ResMut<'w, client_interaction::movement::MovementTicker>>,
    melee: Option<Res<'w, crate::melee::MeleeTarget>>,
    mining: Option<Res<'w, client_interaction::survival_mining::SurvivalMiningRuntime>>,
}

impl LookGestures<'_, '_> {
    /// Drops every finger and returns aim to the crosshair, as when gameplay loses touch input.
    pub(crate) fn reset(&mut self) {
        self.state.gestures.clear();
        self.state.tap = None;
        if let Some(aim) = self.aim.as_mut() {
            aim.0 = None;
        }
    }

    /// Turns this frame's still look-surface fingers into action contacts for the router.
    /// `held` are the fingers still down and `lifted` those that rose this frame. Returns the
    /// break ring to draw: under the finger in tap schemes, or on the crosshair in crosshair
    /// schemes, filled as far as the current break has progressed.
    pub(crate) fn route(
        &mut self,
        scheme: TouchScheme,
        held: &[TouchContact],
        lifted: &[TouchContact],
        targets: &mut SemanticTouchTargets,
    ) -> Option<TouchIndicator> {
        if let Some(movement) = self.movement.as_mut() {
            movement.set_touch_interaction_model(interaction_model(scheme));
        }
        if scheme.shows_action_buttons() {
            self.reset();
            return None;
        }
        let now = self
            .time
            .as_ref()
            .map_or(0.0, |time| time.elapsed_secs_f64());
        let state = &mut *self.state;
        let on_look = |contact: &&TouchContact| contact.hit_id == Some(touch::LOOK_SURFACE);
        let mut live = Vec::new();
        for contact in held.iter().filter(on_look) {
            track(&mut state.gestures, contact, now);
            live.push(contact.contact_id);
        }
        let mut taps = Vec::new();
        for contact in lifted.iter().filter(on_look) {
            track(&mut state.gestures, contact, now);
            taps.extend(state.gestures.end(contact.contact_id, now));
        }
        // Fingers neither held nor lifted were cancelled, or taken by a control.
        state.gestures.retain(|id| live.contains(&id));
        state.gestures.advance(now);
        for (contact_id, position) in state.gestures.holding() {
            targets.pulse(action(contact_id, position, touch::ATTACK));
        }
        let on_actor = self.melee.as_ref().is_some_and(|melee| {
            melee
                .hover
                .is_some_and(|(_, hover)| hover.actor.is_some())
        });
        let tap_action = if on_actor { touch::ATTACK } else { touch::USE };
        for tap in taps {
            targets.pulse(action(tap.contact_id, tap.position, tap_action));
            state.tap = Some((tap.position, TAP_AIM_FRAMES));
        }
        let lingering = state.tap.map(|(position, _)| position);
        state.tap = state
            .tap
            .and_then(|(position, frames)| (frames > 1).then_some((position, frames - 1)));
        if let Some(aim) = self.aim.as_mut() {
            aim.0 = (scheme.aim == TouchAim::Tap)
                .then(|| state.gestures.aim_point().or(lingering))
                .flatten();
        }
        // In tap schemes the ring stays under the newest finger while it is down, looking around
        // included; crosshair schemes show it on the crosshair only while a finger can act.
        let position = match scheme.aim {
            TouchAim::Tap => state.gestures.newest_position()?,
            TouchAim::Crosshair => state.gestures.aim_point().map(|_| [0.5, 0.5])?,
        };
        let breaking = state.gestures.holding().next().is_some();
        Some(TouchIndicator {
            position,
            progress: self
                .mining
                .as_ref()
                .and_then(|mining| mining.destroy_progress())
                .filter(|_| breaking),
        })
    }
}

/// Starts tracking a finger first seen this frame, or follows one already tracked.
fn track(gestures: &mut TouchGestures, contact: &TouchContact, now: f64) {
    if gestures.tracks(contact.contact_id) {
        gestures.move_to(contact.contact_id, contact.position);
    } else {
        gestures.begin(contact.contact_id, contact.position, now);
    }
}

/// A synthetic finger on `hit_id`, routed like a press of that button.
fn action(contact_id: u64, position: [f32; 2], hit_id: u16) -> TouchContact {
    TouchContact {
        contact_id: contact_id | ACTION_CONTACT,
        activity_sequence: 0,
        position,
        delta: [0.0; 2],
        hit_id: Some(hit_id),
    }
}

/// The interaction model Bedrock reports for each scheme. Provisional: the classic schemes'
/// reported model has not been compared with the native client.
fn interaction_model(scheme: TouchScheme) -> protocol::PlayerInteractionModel {
    match (scheme.movement, scheme.aim) {
        (TouchMovement::Dpad, _) => protocol::PlayerInteractionModel::Classic,
        (TouchMovement::Joystick, TouchAim::Tap) => protocol::PlayerInteractionModel::Touch,
        (TouchMovement::Joystick, TouchAim::Crosshair) => {
            protocol::PlayerInteractionModel::Crosshair
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_contacts_never_reuse_a_platform_id() {
        let contact = action(7, [0.5, 0.5], touch::USE);
        assert_ne!(contact.contact_id, 7);
        assert_eq!(contact.contact_id & !ACTION_CONTACT, 7);
        assert_eq!(contact.hit_id, Some(touch::USE));
    }

    #[test]
    fn each_scheme_reports_its_interaction_model() {
        let models: Vec<_> = TouchScheme::ALL.into_iter().map(interaction_model).collect();
        assert_eq!(
            models,
            [
                protocol::PlayerInteractionModel::Classic,
                protocol::PlayerInteractionModel::Classic,
                protocol::PlayerInteractionModel::Touch,
                protocol::PlayerInteractionModel::Crosshair,
            ]
        );
    }
}
