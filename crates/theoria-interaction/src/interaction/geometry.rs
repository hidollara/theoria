use theoria_geometry::geometry::Point;

pub(super) fn point_segment_distance_squared(point: Point, start: Point, end: Point) -> f32 {
    let segment = end - start;
    let length_squared = segment.dot(segment);
    if length_squared == 0.0 {
        return (point - start).dot(point - start);
    }

    let projection = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    let nearest = start + segment * projection;
    let offset = point - nearest;
    offset.dot(offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use theoria_geometry::geometry::Offset;

    #[test]
    fn point_and_vector_operations_preserve_their_meaning() {
        let start = Point::new(10.0, 20.0);
        let offset = Offset::new(3.0, -4.0);
        let end = start + offset;

        assert_eq!(end, Point::new(13.0, 16.0));
        assert_eq!(end - start, offset);
    }

    #[test]
    fn point_segment_distance_clamps_to_the_finite_segment() {
        let start = Point::new(0.0, 0.0);
        let end = Point::new(4.0, 0.0);

        assert_eq!(
            point_segment_distance_squared(Point::new(2.0, 3.0), start, end),
            9.0
        );
        assert_eq!(
            point_segment_distance_squared(Point::new(6.0, 0.0), start, end),
            4.0
        );
    }
}
