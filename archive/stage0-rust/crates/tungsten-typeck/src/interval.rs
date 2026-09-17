#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub min: i64,
    pub max: i64,
}

impl Interval {
    pub fn new(min: i64, max: i64) -> Self {
        Self { min, max }
    }

    pub fn point(val: i64) -> Self {
        Self { min: val, max: val }
    }

    pub fn is_subset_of(&self, other: &Interval) -> bool {
        self.min >= other.min && self.max <= other.max
    }

    pub fn contains(&self, val: i64) -> bool {
        val >= self.min && val <= self.max
    }

    pub fn add(&self, other: &Interval) -> Self {
        Self {
            min: self.min.saturating_add(other.min),
            max: self.max.saturating_add(other.max),
        }
    }

    pub fn sub(&self, other: &Interval) -> Self {
        Self {
            min: self.min.saturating_sub(other.max),
            max: self.max.saturating_sub(other.min),
        }
    }

    pub fn saturating_add(&self, other: &Interval, bound: &Interval) -> Self {
        let raw_min = self.min.saturating_add(other.min);
        let raw_max = self.max.saturating_add(other.max);
        Self {
            min: raw_min.clamp(bound.min, bound.max),
            max: raw_max.clamp(bound.min, bound.max),
        }
    }
}
