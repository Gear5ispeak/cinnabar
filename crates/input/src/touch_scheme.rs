//! Bedrock's four touch control schemes and the look-surface gestures they share.
//!
//! A scheme pairs a movement control, the classic D-pad or the modern joystick, with where
//! actions aim: at the finger (touch) or at the screen-centre crosshair (split). In every scheme a
//! finger on the look surface that stays still is an action: a short tap uses or attacks, and a
//! longer hold breaks. Moving the finger first makes it look instead.

/// The movement control a scheme draws in the joystick's place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TouchMovement {
    /// Eight-way digital arrows.
    Dpad,
    /// The analog stick.
    #[default]
    Joystick,
}

/// Where a scheme's look-surface actions aim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TouchAim {
    /// At the finger, with no crosshair.
    Tap,
    /// At the crosshair ("split controls").
    #[default]
    Crosshair,
}

/// One of the four touch control schemes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TouchScheme {
    pub movement: TouchMovement,
    pub aim: TouchAim,
}

impl TouchScheme {
    /// Every scheme in its persisted order: a saved setting is an index into this list.
    pub const ALL: [Self; 4] = [
        Self::new(TouchMovement::Dpad, TouchAim::Tap),
        Self::new(TouchMovement::Dpad, TouchAim::Crosshair),
        Self::new(TouchMovement::Joystick, TouchAim::Tap),
        Self::new(TouchMovement::Joystick, TouchAim::Crosshair),
    ];

    #[must_use]
    pub const fn new(movement: TouchMovement, aim: TouchAim) -> Self {
        Self { movement, aim }
    }

    /// The scheme a saved index names; an unknown index keeps the default.
    #[must_use]
    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    /// The persisted index of this scheme.
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|scheme| *scheme == self)
            .expect("every scheme is listed")
    }

    /// Only the modern split scheme keeps dedicated attack and use buttons.
    #[must_use]
    pub fn shows_action_buttons(self) -> bool {
        self == Self::default()
    }

    #[must_use]
    pub fn shows_crosshair(self) -> bool {
        self.aim == TouchAim::Crosshair
    }
}

/// What a finger on the classic D-pad presses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DpadPress {
    /// The centre diamond.
    Jump,
    /// An arrow, as a full-speed movement vector with positive Y forward.
    Move([f32; 2]),
    /// A back corner, which has no arrow.
    Nothing,
}

/// The D-pad is a 3x3 grid over the stick's square: arrows on the edges, jump in the centre,
/// and diagonals only in the two forward corners, as on Bedrock's classic controls.
#[must_use]
pub fn dpad_press(deflection: [f32; 2]) -> DpadPress {
    let cell = |value: f32| -> i8 {
        if !value.is_finite() || value.abs() <= 1.0 / 3.0 {
            0
        } else if value > 0.0 {
            1
        } else {
            -1
        }
    };
    let diagonal = std::f32::consts::FRAC_1_SQRT_2;
    match (cell(deflection[0]), cell(deflection[1])) {
        (0, 0) => DpadPress::Jump,
        (x, 0) => DpadPress::Move([f32::from(x), 0.0]),
        (0, y) => DpadPress::Move([0.0, f32::from(y)]),
        (_, -1) => DpadPress::Nothing,
        (x, _) => DpadPress::Move([f32::from(x) * diagonal, diagonal]),
    }
}

/// Seconds a still finger is held before it starts breaking. Provisional: not yet measured
/// against the native client.
pub const HOLD_SECONDS: f64 = 0.3;
/// Viewport fraction a finger may travel and still count as still.
pub const DRAG_DISTANCE: f32 = 0.02;

/// Where a touch action aims and how far its break has progressed, for the ring the touch
/// controls draw there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchIndicator {
    /// Viewport fraction from the top left.
    pub position: [f32; 2],
    /// Break progress from 0 to 1; none draws the empty ring.
    pub progress: Option<f32>,
}

/// What a look-surface finger currently does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GesturePhase {
    /// Still and not yet held long enough to break.
    Pending,
    /// Held still past [`HOLD_SECONDS`]: breaking until it lifts, even if it then moves.
    Holding,
    /// Moved first, so it only looks.
    Looking,
}

#[derive(Clone, Copy, Debug)]
struct Gesture {
    contact_id: u64,
    started: f64,
    origin: [f32; 2],
    position: [f32; 2],
    phase: GesturePhase,
}

/// A finger that lifted while still pending.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tap {
    pub contact_id: u64,
    pub position: [f32; 2],
}

/// Tracks each look-surface finger from press to lift. Callers supply `now` in seconds.
#[derive(Clone, Debug, Default)]
pub struct TouchGestures {
    gestures: Vec<Gesture>,
}

impl TouchGestures {
    /// Starts tracking a finger that pressed the look surface.
    pub fn begin(&mut self, contact_id: u64, position: [f32; 2], now: f64) {
        self.gestures.retain(|gesture| gesture.contact_id != contact_id);
        self.gestures.push(Gesture {
            contact_id,
            started: now,
            origin: position,
            position,
            phase: GesturePhase::Pending,
        });
    }

    /// Moves a tracked finger; a pending finger that travels too far becomes a look.
    pub fn move_to(&mut self, contact_id: u64, position: [f32; 2]) {
        if let Some(gesture) = self.find(contact_id) {
            gesture.position = position;
            let travel = (position[0] - gesture.origin[0]).hypot(position[1] - gesture.origin[1]);
            if gesture.phase == GesturePhase::Pending && travel > DRAG_DISTANCE {
                gesture.phase = GesturePhase::Looking;
            }
        }
    }

    /// Promotes pending fingers held long enough into holds.
    pub fn advance(&mut self, now: f64) {
        for gesture in &mut self.gestures {
            if gesture.phase == GesturePhase::Pending && now - gesture.started >= HOLD_SECONDS {
                gesture.phase = GesturePhase::Holding;
            }
        }
    }

    /// Stops tracking a lifted finger and returns a tap when it never moved or held.
    pub fn end(&mut self, contact_id: u64, now: f64) -> Option<Tap> {
        self.advance(now);
        let index = self
            .gestures
            .iter()
            .position(|gesture| gesture.contact_id == contact_id)?;
        let gesture = self.gestures.remove(index);
        (gesture.phase == GesturePhase::Pending).then_some(Tap {
            contact_id,
            position: gesture.position,
        })
    }

    /// Whether a finger is tracked.
    #[must_use]
    pub fn tracks(&self, contact_id: u64) -> bool {
        self.gestures
            .iter()
            .any(|gesture| gesture.contact_id == contact_id)
    }

    /// Forgets every finger `keep` rejects, without taps.
    pub fn retain(&mut self, mut keep: impl FnMut(u64) -> bool) {
        self.gestures.retain(|gesture| keep(gesture.contact_id));
    }

    /// Forgets a finger without a tap, as when the system cancels it.
    pub fn cancel(&mut self, contact_id: u64) {
        self.gestures.retain(|gesture| gesture.contact_id != contact_id);
    }

    pub fn clear(&mut self) {
        self.gestures.clear();
    }

    /// Fingers currently breaking.
    pub fn holding(&self) -> impl Iterator<Item = (u64, [f32; 2])> + '_ {
        self.gestures
            .iter()
            .filter(|gesture| gesture.phase == GesturePhase::Holding)
            .map(|gesture| (gesture.contact_id, gesture.position))
    }

    /// Where a tap-aimed action points: the newest finger that may still act.
    #[must_use]
    pub fn aim_point(&self) -> Option<[f32; 2]> {
        self.gestures
            .iter()
            .rev()
            .find(|gesture| gesture.phase != GesturePhase::Looking)
            .map(|gesture| gesture.position)
    }

    fn find(&mut self, contact_id: u64) -> Option<&mut Gesture> {
        self.gestures
            .iter_mut()
            .find(|gesture| gesture.contact_id == contact_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemes_round_trip_their_saved_index_and_default_to_modern_split() {
        for (index, scheme) in TouchScheme::ALL.into_iter().enumerate() {
            assert_eq!(TouchScheme::from_index(index), scheme);
            assert_eq!(scheme.index(), index);
        }
        assert_eq!(TouchScheme::from_index(99), TouchScheme::default());
        assert_eq!(
            TouchScheme::default(),
            TouchScheme::new(TouchMovement::Joystick, TouchAim::Crosshair)
        );
        let buttons: Vec<_> = TouchScheme::ALL
            .into_iter()
            .filter(|scheme| scheme.shows_action_buttons())
            .collect();
        assert_eq!(buttons, [TouchScheme::default()]);
    }

    #[test]
    fn the_dpad_grid_jumps_in_the_centre_and_has_only_forward_diagonals() {
        assert_eq!(dpad_press([0.1, 0.1]), DpadPress::Jump);
        assert_eq!(dpad_press([0.1, 0.9]), DpadPress::Move([0.0, 1.0]));
        assert_eq!(dpad_press([-0.8, 0.05]), DpadPress::Move([-1.0, 0.0]));
        assert_eq!(dpad_press([0.0, -0.5]), DpadPress::Move([0.0, -1.0]));
        let DpadPress::Move([x, y]) = dpad_press([0.6, 0.55]) else {
            panic!("the forward corner moves diagonally");
        };
        assert!((x - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
        assert!((y - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
        assert_eq!(dpad_press([-0.6, -0.6]), DpadPress::Nothing);
        assert_eq!(dpad_press([0.6, -0.6]), DpadPress::Nothing);
        assert_eq!(dpad_press([f32::NAN, 0.1]), DpadPress::Jump);
    }

    #[test]
    fn a_still_short_press_taps_and_a_still_long_press_holds() {
        let mut gestures = TouchGestures::default();
        gestures.begin(1, [0.5, 0.5], 0.0);
        assert_eq!(gestures.aim_point(), Some([0.5, 0.5]));
        assert_eq!(
            gestures.end(1, 0.1),
            Some(Tap {
                contact_id: 1,
                position: [0.5, 0.5]
            })
        );

        gestures.begin(2, [0.4, 0.4], 1.0);
        gestures.advance(1.0 + HOLD_SECONDS);
        assert_eq!(gestures.holding().collect::<Vec<_>>(), [(2, [0.4, 0.4])]);
        // A held finger keeps breaking while it moves, and lifting it is no tap.
        gestures.move_to(2, [0.6, 0.4]);
        assert_eq!(gestures.holding().count(), 1);
        assert_eq!(gestures.end(2, 2.0), None);
    }

    #[test]
    fn moving_before_the_hold_only_looks() {
        let mut gestures = TouchGestures::default();
        gestures.begin(3, [0.5, 0.5], 0.0);
        gestures.move_to(3, [0.5 + DRAG_DISTANCE * 2.0, 0.5]);
        gestures.advance(HOLD_SECONDS * 2.0);
        assert_eq!(gestures.holding().count(), 0);
        assert_eq!(gestures.aim_point(), None);
        assert_eq!(gestures.end(3, HOLD_SECONDS * 2.0), None);
    }

    #[test]
    fn a_cancelled_finger_never_taps() {
        let mut gestures = TouchGestures::default();
        gestures.begin(4, [0.5, 0.5], 0.0);
        gestures.cancel(4);
        assert_eq!(gestures.end(4, 0.1), None);
    }
}
