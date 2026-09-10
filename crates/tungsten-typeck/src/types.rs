use crate::interval::Interval;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    U8,
    U16,
    U32,
    U64,
    I64,
    Bool,
    String,
    Unit,
    Refined {
        base: Box<Type>,
        interval: Interval,
        name: Option<String>,
    },
    Ref {
        is_mut: bool,
        inner: Box<Type>,
    },
    Struct(String),
}

impl Type {
    pub fn default_interval(&self) -> Option<Interval> {
        match self {
            Type::U8 => Some(Interval::new(0, 255)),
            Type::U16 => Some(Interval::new(0, 65535)),
            Type::U32 => Some(Interval::new(0, u32::MAX as i64)),
            Type::U64 => Some(Interval::new(0, i64::MAX)),
            Type::I64 => Some(Interval::new(i64::MIN, i64::MAX)),
            Type::Refined { interval, .. } => Some(*interval),
            _ => None,
        }
    }

    pub fn base_type(&self) -> &Type {
        match self {
            Type::Refined { base, .. } => base.base_type(),
            _ => self,
        }
    }

    pub fn is_compatible_with(&self, other: &Type) -> bool {
        if self == other {
            return true;
        }
        match (self, other) {
            (Type::Refined { base: b1, interval: i1, .. }, Type::Refined { base: b2, interval: i2, .. }) => {
                b1 == b2 && i1.is_subset_of(i2)
            }
            (Type::Refined { base, .. }, other) => base.is_compatible_with(other),
            (other, Type::Refined { base, .. }) => other.is_compatible_with(base),
            (Type::Ref { is_mut: m1, inner: i1 }, Type::Ref { is_mut: m2, inner: i2 }) => {
                // If expected is immutable reference, mutable reference can be coerced
                (*m1 == *m2 || (!m2 && *m1)) && i1.is_compatible_with(i2)
            }
            // Coercions between integer primitives
            (Type::I64, Type::U64) | (Type::U64, Type::I64) => true,
            (Type::I64, Type::U8) | (Type::U8, Type::I64) => true,
            (Type::U64, Type::U8) | (Type::U8, Type::U64) => true,
            (Type::U32, Type::U64) | (Type::U32, Type::U8) | (Type::I64, Type::U32) => true,
            _ => false,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::U8 => write!(f, "u8"),
            Type::U16 => write!(f, "u16"),
            Type::U32 => write!(f, "u32"),
            Type::U64 => write!(f, "u64"),
            Type::I64 => write!(f, "i64"),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "String"),
            Type::Unit => write!(f, "()"),
            Type::Refined { base, interval, name } => {
                if let Some(n) = name {
                    write!(f, "{} ({}({}..={}))", n, base, interval.min, interval.max)
                } else {
                    write!(f, "{}({}..={})", base, interval.min, interval.max)
                }
            }
            Type::Ref { is_mut, inner } => {
                if *is_mut {
                    write!(f, "&mut {}", inner)
                } else {
                    write!(f, "&{}", inner)
                }
            }
            Type::Struct(name) => write!(f, "{}", name),
        }
    }
}
