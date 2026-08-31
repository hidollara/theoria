use std::sync::Arc;

use crate::{geometry::Point, transform::Projective2};

use super::HitRegion;

enum Node<T> {
    Empty,
    Target {
        target: T,
        region: HitRegion,
    },
    Transform {
        content: InteractionTree<T>,
        local_to_parent: Projective2,
    },
    Group(Arc<Vec<InteractionTree<T>>>),
}

/// Immutable spatial structure used to locate an interaction target.
///
/// Child order is back-to-front: hit testing examines the last child first.
/// Cloning a tree shares its structure and does not clone `T`.
pub struct InteractionTree<T> {
    node: Arc<Node<T>>,
}

impl<T> InteractionTree<T> {
    pub fn empty() -> Self {
        Self {
            node: Arc::new(Node::Empty),
        }
    }

    pub fn target(target: T, region: HitRegion) -> Self {
        Self {
            node: Arc::new(Node::Target { target, region }),
        }
    }

    /// Groups children in back-to-front order.
    pub fn group(children: impl IntoIterator<Item = Self>) -> Self {
        let children: Vec<_> = children.into_iter().collect();
        if children.is_empty() {
            return Self::empty();
        }
        Self {
            node: Arc::new(Node::Group(Arc::new(children))),
        }
    }

    /// Places local interaction geometry in its parent's coordinate space.
    pub fn transformed(self, local_to_parent: impl Into<Projective2>) -> Self {
        Self {
            node: Arc::new(Node::Transform {
                content: self,
                local_to_parent: local_to_parent.into(),
            }),
        }
    }

    /// Places `foreground` in front of this tree.
    pub fn over(self, foreground: Self) -> Self {
        Self::group([self, foreground])
    }

    /// Returns the frontmost target containing `point`.
    ///
    /// `tolerance` is expressed in this tree's coordinate space. It is mapped
    /// through each inverse transform before curved local geometry is tested.
    pub fn hit_test(&self, point: Point, tolerance: f32) -> Option<Hit<'_, T>> {
        if !point.is_finite() || !tolerance.is_finite() || tolerance <= 0.0 {
            return None;
        }
        self.hit_test_valid(point, tolerance)
    }

    fn hit_test_valid(&self, point: Point, tolerance: f32) -> Option<Hit<'_, T>> {
        match self.node.as_ref() {
            Node::Empty => None,
            Node::Target { target, region } => region.contains(point, tolerance).then_some(Hit {
                target,
                local_position: point,
            }),
            Node::Transform {
                content,
                local_to_parent,
            } => {
                let parent_to_local = local_to_parent.try_inverse()?;
                let local_position = parent_to_local.try_transform_point(point)?;
                let local_scale = maximum_scale_at(parent_to_local, point)?;
                let local_tolerance = tolerance * local_scale;
                if !local_tolerance.is_finite() || local_tolerance <= 0.0 {
                    return None;
                }
                content.hit_test_valid(local_position, local_tolerance)
            }
            Node::Group(children) => children
                .iter()
                .rev()
                .find_map(|child| child.hit_test_valid(point, tolerance)),
        }
    }
}

impl<T> Clone for InteractionTree<T> {
    fn clone(&self) -> Self {
        Self {
            node: Arc::clone(&self.node),
        }
    }
}

impl<T> Default for InteractionTree<T> {
    fn default() -> Self {
        Self::empty()
    }
}

/// One target and the tested point expressed in that target's local space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit<'tree, T> {
    pub target: &'tree T,
    pub local_position: Point,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        math::Vector2,
        path::{FillRule, Path},
        transform::Affine2,
    };

    fn square(target: &'static str, size: f32) -> InteractionTree<&'static str> {
        let path = Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(size, 0.0),
            Point::new(size, size),
            Point::new(0.0, size),
        ]);
        InteractionTree::target(target, HitRegion::fill(path, FillRule::NonZero))
    }

    #[test]
    fn the_last_overlapping_child_is_frontmost() {
        let tree = InteractionTree::group([square("back", 10.0), square("front", 10.0)]);

        assert_eq!(
            tree.hit_test(Point::new(5.0, 5.0), 0.01)
                .map(|hit| *hit.target),
            Some("front")
        );
    }

    #[test]
    fn transforms_are_inverted_into_target_local_coordinates() {
        let tree = square("translated", 10.0)
            .transformed(Affine2::from_translation(Vector2::new(100.0, 50.0)));
        let hit = tree.hit_test(Point::new(104.0, 57.0), 0.01).unwrap();

        assert_eq!(*hit.target, "translated");
        assert_eq!(hit.local_position, Point::new(4.0, 7.0));
        assert!(tree.hit_test(Point::new(4.0, 7.0), 0.01).is_none());
    }

    #[test]
    fn nested_transforms_compose_during_descent() {
        let tree = square("nested", 1.0)
            .transformed(Affine2::from_scale(Vector2::new(10.0, 20.0)))
            .transformed(Affine2::from_translation(Vector2::new(5.0, 7.0)));
        let hit = tree.hit_test(Point::new(10.0, 17.0), 0.01).unwrap();

        assert!((hit.local_position.x - 0.5).abs() < 0.0001);
        assert!((hit.local_position.y - 0.5).abs() < 0.0001);
    }

    #[test]
    fn singular_transforms_cannot_produce_a_hit() {
        let tree =
            square("collapsed", 10.0).transformed(Affine2::from_scale(Vector2::new(0.0, 1.0)));

        assert!(tree.hit_test(Point::new(0.0, 5.0), 0.01).is_none());
    }

    #[test]
    fn cloned_trees_share_non_clone_targets() {
        struct Target;

        let region = HitRegion::fill(
            Path::polygon([
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(0.0, 1.0),
            ]),
            FillRule::NonZero,
        );
        let tree = InteractionTree::target(Target, region);
        let clone = tree.clone();

        assert!(clone.hit_test(Point::new(0.1, 0.1), 0.01).is_some());
    }
}
