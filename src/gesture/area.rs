use crate::{
    component::UpdateContext, event::EventTarget, geometry::Point, interaction::HitRegion,
    transform::Projective2,
};

#[derive(Clone, Debug)]
pub(super) struct GestureArea<E: 'static> {
    sink: EventTarget<E>,
    region: HitRegion,
    local_to_parent: Projective2,
}

impl<E: 'static> GestureArea<E> {
    pub(super) fn new(sink: EventTarget<E>, region: HitRegion) -> Self {
        Self {
            sink,
            region,
            local_to_parent: Projective2::IDENTITY,
        }
    }

    pub(super) fn transformed(mut self, local_to_parent: impl Into<Projective2>) -> Self {
        self.local_to_parent = self.local_to_parent.then(local_to_parent.into());
        self
    }

    pub(super) fn contains(&self, point: Point, tolerance: f32) -> bool {
        self.hit(point, tolerance).is_some()
    }

    pub(super) fn map_point(&self, point: Point) -> Option<Point> {
        if !point.is_finite() {
            return None;
        }
        self.local_to_parent
            .try_inverse()?
            .try_transform_point(point)
    }

    pub(super) fn send(&self, event: E, context: &mut UpdateContext<'_>) -> bool {
        context.send(&self.sink, event).is_ok()
    }

    pub(super) fn hit(&self, point: Point, tolerance: f32) -> Option<Point> {
        if !point.is_finite() || !tolerance.is_finite() || tolerance <= 0.0 {
            return None;
        }

        let parent_to_local = self.local_to_parent.try_inverse()?;
        let local_position = parent_to_local.try_transform_point(point)?;
        let local_scale = maximum_scale_at(parent_to_local, point)?;
        let local_tolerance = tolerance * local_scale;
        if !local_tolerance.is_finite() || local_tolerance <= 0.0 {
            return None;
        }
        self.region
            .contains(local_position, local_tolerance)
            .then_some(local_position)
    }
}

fn maximum_scale_at(transform: Projective2, point: Point) -> Option<f32> {
    let [[a, b, c], [d, e, f], [g, h, i]] = transform.rows();
    let numerator_x = a * point.x + b * point.y + c;
    let numerator_y = d * point.x + e * point.y + f;
    let denominator = g * point.x + h * point.y + i;
    if !denominator.is_finite() || denominator.abs() <= f32::EPSILON {
        return None;
    }

    let denominator_squared = denominator * denominator;
    let j00 = (a * denominator - g * numerator_x) / denominator_squared;
    let j01 = (b * denominator - h * numerator_x) / denominator_squared;
    let j10 = (d * denominator - g * numerator_y) / denominator_squared;
    let j11 = (e * denominator - h * numerator_y) / denominator_squared;
    if ![j00, j01, j10, j11].into_iter().all(f32::is_finite) {
        return None;
    }

    let column0_squared = j00 * j00 + j10 * j10;
    let column1_squared = j01 * j01 + j11 * j11;
    let column_dot = j00 * j01 + j10 * j11;
    let discriminant =
        ((column0_squared - column1_squared).powi(2) + 4.0 * column_dot * column_dot).sqrt();
    let largest_eigenvalue = 0.5 * (column0_squared + column1_squared + discriminant);
    let scale = largest_eigenvalue.max(0.0).sqrt();
    scale.is_finite().then_some(scale)
}
