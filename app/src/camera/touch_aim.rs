//! Tap-aimed touch schemes point the interaction ray through the finger instead of the crosshair.

use bevy::prelude::*;
use client_presentation::{camera::FlyCamera, local_player::InteractionOriginSnapshot};

/// Where tap-aimed touch actions point this frame, as a viewport fraction from the top left;
/// `None` leaves them on the crosshair.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct TouchAimPoint(pub(crate) Option<[f32; 2]>);

/// Redirects this frame's interaction ray, and the unsent tick's reported aim, through the
/// touched point. Runs after server aim assistance, which a finger overrides.
pub(crate) fn aim_at_touch(
    point: Res<TouchAimPoint>,
    cameras: Query<&Projection, With<FlyCamera>>,
    mut interaction: ResMut<InteractionOriginSnapshot>,
    mut movement: ResMut<client_interaction::movement::MovementTicker>,
) {
    let Some(point) = point.0 else {
        return;
    };
    let Some(ray) = interaction.outbound_ray() else {
        return;
    };
    let Ok(Projection::Perspective(projection)) = cameras.single() else {
        return;
    };
    let Some(direction) = through_point(
        ray.direction(),
        projection.fov,
        projection.aspect_ratio,
        point,
    ) else {
        return;
    };
    let tick = ray.physics_tick().get();
    interaction.assist_direction(direction);
    movement.publish_interaction_aim(tick, Some(direction.to_array()));
}

/// The world direction through a viewport `point` for an unrolled camera looking along
/// `forward` with a vertical field of view in radians.
fn through_point(forward: Vec3, vertical_fov: f32, aspect: f32, point: [f32; 2]) -> Option<Vec3> {
    let rotation = Transform::IDENTITY.looking_to(forward, Vec3::Y).rotation;
    let half_height = (vertical_fov * 0.5).tan();
    let x = (point[0] * 2.0 - 1.0) * half_height * aspect;
    let y = (1.0 - point[1] * 2.0) * half_height;
    (rotation * Vec3::new(x, y, -1.0))
        .try_normalize()
        .filter(|direction| direction.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn the_screen_centre_keeps_the_crosshair_direction() {
        let forward = Vec3::new(0.3, -0.2, -0.9).normalize();
        let direction = through_point(forward, 1.2, 16.0 / 9.0, [0.5, 0.5]).unwrap();
        assert!(close(direction, forward));
    }

    #[test]
    fn edges_follow_the_field_of_view() {
        // A 90 degree square view puts each edge 45 degrees off the forward axis.
        let right = through_point(Vec3::NEG_Z, FRAC_PI_2, 1.0, [1.0, 0.5]).unwrap();
        assert!(close(right, Vec3::new(1.0, 0.0, -1.0).normalize()));
        let top = through_point(Vec3::NEG_Z, FRAC_PI_2, 1.0, [0.5, 0.0]).unwrap();
        assert!(close(top, Vec3::new(0.0, 1.0, -1.0).normalize()));
    }
}
