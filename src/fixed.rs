/// Signed Q16.16 value. Arithmetic is checked; division truncates toward zero.
/// The raw bits, rather than floating-point conversions, are authoritative.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Fixed(i32);

impl Fixed {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1 << 16);
    pub const fn from_bits(bits: i32) -> Self {
        Self(bits)
    }
    pub const fn bits(self) -> i32 {
        self.0
    }
    pub fn from_int(value: i32) -> Option<Self> {
        i32::try_from(i64::from(value) * 65536).ok().map(Self)
    }
    pub fn from_ratio(numerator: i32, denominator: i32) -> Option<Self> {
        if denominator == 0 {
            return None;
        }
        i32::try_from(i64::from(numerator) * 65536 / i64::from(denominator))
            .ok()
            .map(Self)
    }
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Self)
    }
    pub fn checked_mul(self, rhs: Self) -> Option<Self> {
        i32::try_from(i64::from(self.0) * i64::from(rhs.0) / 65536)
            .ok()
            .map(Self)
    }
    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        Self::from_ratio(self.0, rhs.0)
    }
    /// Presentation only. Never feed this value back into authoritative state.
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / 65536.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundaries_and_rounding() {
        assert!(Fixed::from_int(32768).is_none());
        assert_eq!(Fixed::from_int(-32768).unwrap().bits(), i32::MIN);
        assert_eq!(Fixed::from_ratio(-1, 3).unwrap().bits(), -21845);
        assert_eq!(
            Fixed::from_ratio(3, 2)
                .unwrap()
                .checked_mul(Fixed::from_int(2).unwrap()),
            Fixed::from_int(3)
        );
        assert_eq!(Fixed::ONE.checked_div(Fixed::ZERO), None);
        assert!(Fixed::from_bits(i32::MAX).checked_add(Fixed::ONE).is_none());
        assert!(
            Fixed::from_bits(i32::MIN)
                .checked_div(Fixed::from_bits(-1))
                .is_none()
        );
    }
}
