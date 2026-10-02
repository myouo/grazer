use crate::Fixed;

/// Authoritative Q16.16 position, velocity or local shape offset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Vec2 {
    pub x: Fixed,
    pub y: Fixed,
}
impl Vec2 {
    pub const ZERO: Self = Self::new(Fixed::ZERO, Fixed::ZERO);
    pub const fn new(x: Fixed, y: Fixed) -> Self {
        Self { x, y }
    }
    pub fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self::new(
            self.x.checked_add(other.x)?,
            self.y.checked_add(other.y)?,
        ))
    }
    fn raw(self) -> Point {
        Point(i64::from(self.x.bits()), i64::from(self.y.bits()))
    }
}

pub const MAX_CURVE_POINTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCollider;
impl std::fmt::Display for InvalidCollider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("collider needs a positive radius and curves need 2..=16 points")
    }
}
impl std::error::Error for InvalidCollider {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    Circle,
    Capsule {
        start: Vec2,
        end: Vec2,
    },
    Curve {
        points: [Vec2; MAX_CURVE_POINTS],
        len: u8,
    },
}

/// A circle, capsule or thick polyline in local coordinates. Shapes translate
/// rigidly during a tick; rotation and deformation are later motion features.
/// Curves are the union of their segment capsules, including round end caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Collider {
    pub(crate) shape: Shape,
    radius: Fixed,
    // Local bounds including thickness, cached once at construction.
    bounds: [i64; 4],
}
impl Collider {
    pub fn circle(radius: Fixed) -> Result<Self, InvalidCollider> {
        Self::build(Shape::Circle, radius)
    }
    pub fn capsule(start: Vec2, end: Vec2, radius: Fixed) -> Result<Self, InvalidCollider> {
        Self::build(Shape::Capsule { start, end }, radius)
    }
    pub fn curve(points: &[Vec2], radius: Fixed) -> Result<Self, InvalidCollider> {
        if !(2..=MAX_CURVE_POINTS).contains(&points.len()) {
            return Err(InvalidCollider);
        }
        let mut storage = [Vec2::ZERO; MAX_CURVE_POINTS];
        storage[..points.len()].copy_from_slice(points);
        Self::build(
            Shape::Curve {
                points: storage,
                len: points.len() as u8,
            },
            radius,
        )
    }
    fn build(shape: Shape, radius: Fixed) -> Result<Self, InvalidCollider> {
        if radius.bits() <= 0 {
            return Err(InvalidCollider);
        }
        let mut bounds = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
        let mut include = |p: Vec2| {
            let Point(x, y) = p.raw();
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x);
            bounds[3] = bounds[3].max(y);
        };
        match shape {
            Shape::Circle => include(Vec2::ZERO),
            Shape::Capsule { start, end } => {
                include(start);
                include(end);
            }
            Shape::Curve { points, len } => {
                for &point in &points[..usize::from(len)] {
                    include(point);
                }
            }
        }
        let r = i64::from(radius.bits());
        bounds[0] -= r;
        bounds[1] -= r;
        bounds[2] += r;
        bounds[3] += r;
        Ok(Self {
            shape,
            radius,
            bounds,
        })
    }
    pub fn radius(&self) -> Fixed {
        self.radius
    }
    /// Local endpoints of capsule/polyline segments, in collision order.
    pub fn segments(&self) -> impl ExactSizeIterator<Item = (Vec2, Vec2)> + '_ {
        let len = match self.shape {
            Shape::Circle => 0,
            Shape::Capsule { .. } => 1,
            Shape::Curve { len, .. } => usize::from(len) - 1,
        };
        (0..len).map(|i| match self.shape {
            Shape::Capsule { start, end } => (start, end),
            Shape::Curve { points, .. } => (points[i], points[i + 1]),
            Shape::Circle => unreachable!("circle has no segments"),
        })
    }

    /// Inclusive contact with a circle. All arithmetic uses integer raw bits.
    pub fn intersects_circle(&self, position: Vec2, center: Vec2, radius: Fixed) -> bool {
        self.swept_contact(position, position, center, center, radius)
    }

    /// Continuous collision for two linear translations over the same tick.
    /// Both endpoints and exact tangency count as contact. A negative target
    /// radius is rejected. There is no sampling, epsilon or floating-point math.
    pub fn swept_contact(
        &self,
        previous: Vec2,
        current: Vec2,
        target_previous: Vec2,
        target_current: Vec2,
        target_radius: Fixed,
    ) -> bool {
        if target_radius.bits() < 0 {
            return false;
        }
        let a = target_previous.raw() - previous.raw();
        let b = target_current.raw() - current.raw();
        let extra = i64::from(target_radius.bits());
        if a.0.max(b.0) < self.bounds[0] - extra
            || a.0.min(b.0) > self.bounds[2] + extra
            || a.1.max(b.1) < self.bounds[1] - extra
            || a.1.min(b.1) > self.bounds[3] + extra
        {
            return false;
        }
        let radius = i64::from(self.radius.bits()) + extra;
        match &self.shape {
            Shape::Circle => point_near_segment(Point(0, 0), a, b, radius),
            Shape::Capsule { start, end } => segments_near(a, b, start.raw(), end.raw(), radius),
            Shape::Curve { points, len } => points[..usize::from(*len)]
                .windows(2)
                .any(|pair| segments_near(a, b, pair[0].raw(), pair[1].raw(), radius)),
        }
    }
    pub(crate) fn outside(&self, position: Vec2, width: Fixed, height: Fixed) -> bool {
        let Point(x, y) = position.raw();
        x + self.bounds[2] < 0
            || y + self.bounds[3] < 0
            || x + self.bounds[0] >= i64::from(width.bits())
            || y + self.bounds[1] >= i64::from(height.bits())
    }
}

#[derive(Clone, Copy)]
struct Point(i64, i64);
impl std::ops::Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0, self.1 - rhs.1)
    }
}
fn dot(a: Point, b: Point) -> i128 {
    i128::from(a.0) * i128::from(b.0) + i128::from(a.1) * i128::from(b.1)
}
fn cross(a: Point, b: Point) -> i128 {
    i128::from(a.0) * i128::from(b.1) - i128::from(a.1) * i128::from(b.0)
}

// Return high/low halves of the exact unsigned 256-bit product. Q16.16
// endpoint differences fit i64, but squaring a cross product can exceed i128.
fn wide_product(a: u128, b: u128) -> (u128, u128) {
    let mask = u128::from(u64::MAX);
    let (a0, a1) = (a & mask, a >> 64);
    let (b0, b1) = (b & mask, b >> 64);
    let low = a0 * b0;
    let middle = (low >> 64) + ((a1 * b0) & mask) + ((a0 * b1) & mask);
    let high = a1 * b1 + ((a1 * b0) >> 64) + ((a0 * b1) >> 64) + (middle >> 64);
    (high, (middle << 64) | (low & mask))
}
fn point_near_segment(point: Point, start: Point, end: Point, radius: i64) -> bool {
    let edge = end - start;
    let offset = point - start;
    let length = dot(edge, edge);
    let projection = dot(offset, edge);
    let radius_squared = i128::from(radius) * i128::from(radius);
    if length == 0 || projection <= 0 {
        return dot(offset, offset) <= radius_squared;
    }
    if projection >= length {
        let d = point - end;
        return dot(d, d) <= radius_squared;
    }
    let area = cross(offset, edge).unsigned_abs();
    wide_product(area, area) <= wide_product(radius_squared as u128, length as u128)
}
fn segments_near(a: Point, b: Point, c: Point, d: Point, radius: i64) -> bool {
    let ab = b - a;
    let cd = d - c;
    let sides = [
        cross(ab, c - a),
        cross(ab, d - a),
        cross(cd, a - c),
        cross(cd, b - c),
    ];
    // Bounding-box check is necessary for disjoint collinear segments.
    let overlaps = a.0.min(b.0) <= c.0.max(d.0)
        && c.0.min(d.0) <= a.0.max(b.0)
        && a.1.min(b.1) <= c.1.max(d.1)
        && c.1.min(d.1) <= a.1.max(b.1);
    let straddles = |x: i128, y: i128| x == 0 || y == 0 || (x < 0) != (y < 0);
    (overlaps && straddles(sides[0], sides[1]) && straddles(sides[2], sides[3]))
        || point_near_segment(a, c, d, radius)
        || point_near_segment(b, c, d, radius)
        || point_near_segment(c, a, b, radius)
        || point_near_segment(d, a, b, radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiplication_handles_full_width() {
        assert_eq!(wide_product(u128::MAX, u128::MAX), (u128::MAX - 1, 1));
        assert_eq!(wide_product(1 << 127, 2), (1, 0));
        assert_eq!(
            wide_product(u64::MAX.into(), u64::MAX.into()),
            (0, u128::from(u64::MAX).pow(2))
        );
    }
}
