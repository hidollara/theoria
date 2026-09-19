//! Distance and normalized-position sampling along flattened Paths.

use crate::{geometry::Point, path::Path, path::flatten_subpaths};

pub(crate) struct PathSampler {
    subpaths: Vec<MeasuredSubpath>,
}

struct MeasuredSubpath {
    segments: Vec<MeasuredSegment>,
    length: f32,
}

struct MeasuredSegment {
    from: Point,
    to: Point,
    start: f32,
    length: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PathSample {
    offset: f32,
    pub(crate) signed_distance: f32,
}

impl PathSampler {
    pub(crate) fn new(path: &Path, tolerance: f32) -> Self {
        let subpaths = flatten_subpaths(path, tolerance)
            .into_iter()
            .map(|points| {
                let mut length = 0.0;
                let segments = points
                    .windows(2)
                    .filter_map(|pair| {
                        let from = pair[0];
                        let to = pair[1];
                        let segment_length = (to - from).length();
                        if segment_length == 0.0 || !segment_length.is_finite() {
                            return None;
                        }
                        let segment = MeasuredSegment {
                            from,
                            to,
                            start: length,
                            length: segment_length,
                        };
                        length += segment_length;
                        Some(segment)
                    })
                    .collect();
                MeasuredSubpath { segments, length }
            })
            .filter(|subpath| subpath.length > 0.0)
            .collect();
        Self { subpaths }
    }

    pub(crate) fn nearest_offset(&self, point: Point) -> Option<f32> {
        self.nearest_sample(point).map(|sample| sample.offset)
    }

    pub(crate) fn nearest_sample(&self, point: Point) -> Option<PathSample> {
        let mut nearest: Option<(f32, PathSample)> = None;
        for subpath in &self.subpaths {
            for segment in &subpath.segments {
                let direction = segment.to - segment.from;
                let amount = ((point - segment.from).dot(direction)
                    / (segment.length * segment.length))
                    .clamp(0.0, 1.0);
                let closest = segment.from + direction * amount;
                let displacement = point - closest;
                let distance_squared = displacement.dot(displacement);
                let offset = (segment.start + segment.length * amount) / subpath.length;
                let signed_distance =
                    (direction.x * displacement.y - direction.y * displacement.x) / segment.length;
                let sample = PathSample {
                    offset,
                    signed_distance,
                };
                if nearest.is_none_or(|(best_distance, _)| distance_squared < best_distance) {
                    nearest = Some((distance_squared, sample));
                }
            }
        }
        nearest.map(|(_, sample)| sample)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        geometry::Point,
        path::{Path, Subpath},
        tessellation::DEFAULT_TESSELLATION_TOLERANCE,
    };

    use super::PathSampler;

    #[test]
    fn along_path_offsets_restart_for_each_subpath() {
        let path = Path::from_subpaths([
            Subpath::new(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)),
            Subpath::new(Point::new(0.0, 20.0)).line_to(Point::new(100.0, 20.0)),
        ]);
        let sampler = PathSampler::new(&path, DEFAULT_TESSELLATION_TOLERANCE);

        assert!((sampler.nearest_offset(Point::new(2.5, 1.0)).unwrap() - 0.25).abs() < 0.001);
        assert!((sampler.nearest_offset(Point::new(25.0, 19.0)).unwrap() - 0.25).abs() < 0.001);
        assert!(
            (sampler
                .nearest_sample(Point::new(2.5, 1.0))
                .unwrap()
                .signed_distance
                - 1.0)
                .abs()
                < 0.001
        );
        assert!(
            (sampler
                .nearest_sample(Point::new(25.0, 19.0))
                .unwrap()
                .signed_distance
                + 1.0)
                .abs()
                < 0.001
        );
    }
}
