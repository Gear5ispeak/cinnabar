//! The touch break indicator: a ring where an action aims, filled from its centre as the block
//! being broken cracks, like Bedrock's touch controls.

use super::*;
use crate::ui_runtime::presentation::primitives::rect;
use semantic_input::TouchIndicator;
use ui::{UiBlendMode, UiMesh, UiMeshBatch, UiMeshVertex, UiNodeId, UiVisual};

/// Ring radius as a share of the screen height.
const RADIUS: f32 = 0.045;
/// Ring stroke width as a share of the ring radius.
const STROKE: f32 = 0.14;
/// Segments approximating each circle.
const SEGMENTS: u32 = 48;
const RING_COLOR: [u8; 4] = [255, 255, 255, 220];
const FILL_COLOR: [u8; 4] = [255, 255, 255, 235];

impl UiPresentationRuntime {
    /// Shows or hides the break indicator for the next paint.
    pub fn set_touch_indicator(&mut self, indicator: Option<TouchIndicator>) {
        self.screens.touch.indicator = indicator;
    }
}

/// Paints `indicator` above the touch controls through `paint`, whose node origin is the
/// safe-area content's top-left.
pub(super) fn append(
    paint: &mut PaintFrame<'_>,
    indicator: TouchIndicator,
) -> Result<(), UiPresentationError> {
    let safe = paint.safe_area;
    let content = paint.content;
    let window = [
        content[0] + safe.left() + safe.right(),
        content[1] + safe.top() + safe.bottom(),
    ];
    if window.iter().any(|side| !side.is_finite() || *side <= 0.0) {
        return Ok(());
    }
    let radius = RADIUS * window[1];
    let centre = [
        indicator.position[0] * window[0] - safe.left(),
        indicator.position[1] * window[1] - safe.top(),
    ];
    let mesh = indicator_mesh(paint.solid_page, indicator.progress)
        .map_err(|_| UiPresentationError::Tree(ui::UiError::DrawIndexOverflow))?;
    paint.nodes.push(
        UiNode::new(
            UiNodeId::new(*paint.next),
            None,
            rect(
                centre[0] - radius,
                centre[1] - radius,
                centre[0] + radius,
                centre[1] + radius,
            )?,
        )
        .with_visual(UiVisual::Mesh(Arc::new(mesh))),
    );
    *paint.next += 1;
    Ok(())
}

/// The ring and, while breaking, a centred disc whose radius grows with `progress` until it
/// meets the ring, in the node's unit square.
fn indicator_mesh(solid_page: u16, progress: Option<f32>) -> Result<UiMesh, ui::UiMeshError> {
    let vertex = |[x, y]: [f32; 2], color| UiMeshVertex {
        position: [x, y],
        clip_z: 0.0,
        clip_w: 1.0,
        uv: [0.5, 0.5],
        color,
        model_light: 1.0,
        overlay_color: [0.0; 4],
        style_flags: 0,
        alpha_test: false,
    };
    let point = |radius: f32, index: u32| {
        let angle = index as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        [0.5 + radius * angle.cos(), 0.5 + radius * angle.sin()]
    };
    let (outer, inner) = (0.5, 0.5 * (1.0 - STROKE));
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for index in 0..SEGMENTS {
        vertices.push(vertex(point(outer, index), RING_COLOR));
        vertices.push(vertex(point(inner, index), RING_COLOR));
        let [a, b] = [index * 2, index * 2 + 1];
        let [c, d] = [(index + 1) % SEGMENTS * 2, (index + 1) % SEGMENTS * 2 + 1];
        indices.extend([a, b, c, b, d, c]);
    }
    if let Some(progress) = progress.filter(|progress| *progress > 0.0) {
        let radius = inner * progress.min(1.0);
        let centre = vertices.len() as u32;
        vertices.push(vertex([0.5, 0.5], FILL_COLOR));
        for index in 0..SEGMENTS {
            vertices.push(vertex(point(radius, index), FILL_COLOR));
            let next = (index + 1) % SEGMENTS;
            indices.extend([centre, centre + 1 + index, centre + 1 + next]);
        }
    }
    let count = indices.len() as u32;
    UiMesh::new(
        vertices.into(),
        indices.into(),
        Arc::from([UiMeshBatch {
            texture_page: solid_page,
            index_range: 0..count,
            blend: UiBlendMode::Alpha,
            depth_test: false,
            depth_write: false,
            alpha_cutoff: None,
        }]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_empty_ring_has_no_fill_and_breaking_adds_one() {
        let ring = indicator_mesh(0, None).unwrap();
        let started = indicator_mesh(0, Some(0.0)).unwrap();
        let breaking = indicator_mesh(0, Some(0.5)).unwrap();
        assert_eq!(ring, started, "no dot before the break progresses");
        assert_ne!(ring, breaking);
    }
}
