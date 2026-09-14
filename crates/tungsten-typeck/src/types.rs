use crate::interval::Interval;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegionId(pub usize);

impl fmt::Display for RegionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'r{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    U8,
    U16,
    U32,
    U64,
    I64,
    Usize,
    Bool,
    String,
    Unit,
    GenericParam(String),
    Refined {
        base: Box<Type>,
        interval: Interval,
        name: Option<String>,
    },
    Relational {
        base: Box<Type>,
        predicate_desc: String,
    },
    Ref {
        is_mut: bool,
        inner: Box<Type>,
        region: Option<RegionId>,
    },

    Struct(String),
    Instantiated {
        name: String,
        args: Vec<Type>,
    },
    Fn {
        params: Vec<Type>,
        return_type: Box<Type>,
        yields_effects: Vec<String>,
    },
}

impl Type {
    pub fn default_interval(&self) -> Option<Interval> {
        match self {
            Type::U8 => Some(Interval::new(0, 255)),
            Type::U16 => Some(Interval::new(0, 65535)),
            Type::U32 => Some(Interval::new(0, u32::MAX as i64)),
            Type::U64 | Type::Usize => Some(Interval::new(0, i64::MAX)),
            Type::I64 => Some(Interval::new(i64::MIN, i64::MAX)),
            Type::Refined { interval, .. } => Some(*interval),
            _ => None,
        }
    }

    pub fn base_type(&self) -> &Type {
        match self {
            Type::Refined { base, .. } => base.base_type(),
            Type::Relational { base, .. } => base.base_type(),
            _ => self,
        }
    }

    pub fn is_compatible_with(&self, other: &Type) -> bool {
        if self == other {
            return true;
        }
        match (self, other) {
            (Type::GenericParam(_), _) | (_, Type::GenericParam(_)) => true,
            (Type::Refined { base: b1, interval: i1, .. }, Type::Refined { base: b2, interval: i2, .. }) => {
                b1 == b2 && i1.is_subset_of(i2)
            }
            (Type::Refined { base, .. }, other) => base.is_compatible_with(other),
            (other, Type::Refined { base, .. }) => other.is_compatible_with(base),
            (Type::Relational { base, .. }, other) => base.is_compatible_with(other),
            (other, Type::Relational { base, .. }) => other.is_compatible_with(base),
            (Type::Instantiated { name: n1, args: a1 }, Type::Instantiated { name: n2, args: a2 }) => {
                n1 == n2 && a1.len() == a2.len() && a1.iter().zip(a2.iter()).all(|(x, y)| x.is_compatible_with(y))
            }
            (Type::Struct(n1), Type::Instantiated { name: n2, .. }) | (Type::Instantiated { name: n1, .. }, Type::Struct(n2)) => {
                n1 == n2
            }
            (Type::Ref { is_mut: m1, inner: i1, region: r1 }, Type::Ref { is_mut: m2, inner: i2, region: r2 }) => {
                let region_compat = match (r1, r2) {
                    (Some(reg1), Some(reg2)) => reg1.0 <= reg2.0, // reg1 is at outer or equal scope, outliving reg2
                    _ => true,
                };
                (*m1 == *m2 || (!m2 && *m1)) && i1.is_compatible_with(i2) && region_compat
            }
            // Coercions between integer primitives
            (Type::I64, Type::U64) | (Type::U64, Type::I64) => true,
            (Type::I64, Type::U8) | (Type::U8, Type::I64) => true,
            (Type::U64, Type::U8) | (Type::U8, Type::U64) => true,
            (Type::Usize, Type::U64) | (Type::U64, Type::Usize) => true,
            (Type::Usize, Type::I64) | (Type::I64, Type::Usize) => true,
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
            Type::Usize => write!(f, "usize"),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "String"),
            Type::Unit => write!(f, "()"),
            Type::GenericParam(name) => write!(f, "{}", name),
            Type::Refined { base, interval, name } => {
                if let Some(n) = name {
                    write!(f, "{} ({}({}..={}))", n, base, interval.min, interval.max)
                } else {
                    write!(f, "{}({}..={})", base, interval.min, interval.max)
                }
            }
            Type::Relational { base, predicate_desc } => {
                write!(f, "{}({})", base, predicate_desc)
            }
            Type::Ref { is_mut, inner, region } => {
                let m_str = if *is_mut { "mut " } else { "" };
                if let Some(r) = region {
                    write!(f, "&{} {}{}", r, m_str, inner)
                } else {
                    write!(f, "&{}{}", m_str, inner)
                }
            }

            Type::Struct(name) => write!(f, "{}", name),
            Type::Instantiated { name, args } => {
                let arg_strs: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                write!(f, "{}<{}>", name, arg_strs.join(", "))
            }
            Type::Fn { params, return_type, yields_effects } => {
                let p_strs: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                write!(f, "fn({})", p_strs.join(", "))?;
                if !yields_effects.is_empty() {
                    write!(f, " yields [{}]", yields_effects.join(", "))?;
                }
                write!(f, " -> {}", return_type)
            }
        }
    }
}
