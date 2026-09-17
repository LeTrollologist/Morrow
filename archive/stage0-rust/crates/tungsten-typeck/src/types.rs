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
    Ptr {
        is_mut: bool,
        inner: Box<Type>,
    },

    Struct(String),
    Enum(String),
    Array {
        elem: Box<Type>,
        len: usize,
    },
    Instantiated {
        name: String,
        args: Vec<Type>,
    },
    Fn {
        params: Vec<Type>,
        return_type: Box<Type>,
        yields_effects: Vec<String>,
    },
    /// A value bounded to a specific region's lexical lifetime.
    /// Values of this type cannot escape outliving their declaring region.
    RegionBounded {
        inner: Box<Type>,
        region: RegionId,
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
            Type::RegionBounded { inner, .. } => inner.base_type(),
            _ => self,
        }
    }

    pub fn region(&self) -> Option<RegionId> {
        match self {
            Type::Ref { region, .. } => *region,
            Type::RegionBounded { region, .. } => Some(*region),
            _ => None,
        }
    }

    pub fn strip_region(&self) -> &Type {
        match self {
            Type::RegionBounded { inner, .. } => inner.strip_region(),
            _ => self,
        }
    }

    pub fn bounded_with(self, reg: RegionId) -> Type {
        match self {
            Type::RegionBounded { inner, region: existing } => {
                Type::RegionBounded {
                    inner,
                    region: RegionId(existing.0.max(reg.0)),
                }
            }
            _ => Type::RegionBounded {
                inner: Box::new(self),
                region: reg,
            },
        }
    }

    pub fn stride(&self) -> usize {
        match self {
            Type::U8 | Type::Bool => 1,
            Type::U16 => 2,
            Type::U32 => 4,
            Type::U64 | Type::I64 | Type::Usize => 8,
            Type::String => 8,
            Type::Unit => 0,
            Type::Refined { base, .. } => base.stride(),
            Type::Relational { base, .. } => base.stride(),
            Type::RegionBounded { inner, .. } => inner.stride(),
            Type::Ref { .. } => 8,
            Type::Ptr { .. } => 8,
            Type::Struct(_) => 8,
            Type::Enum(_) => 8,
            Type::Instantiated { .. } => 8,
            Type::Fn { .. } => 8,
            Type::GenericParam(_) => 8,
            Type::Array { elem, len } => elem.stride() * len,
        }
    }

    pub fn is_compatible_with(&self, other: &Type) -> bool {
        if self == other {
            return true;
        }
        match (self, other) {
            (Type::GenericParam(_), _) | (_, Type::GenericParam(_)) => true,
            (Type::RegionBounded { inner: i1, region: r1 }, Type::RegionBounded { inner: i2, region: r2 }) => {
                r1.0 <= r2.0 && i1.is_compatible_with(i2)
            }
            (Type::RegionBounded { inner, .. }, other) => inner.is_compatible_with(other),
            (other, Type::RegionBounded { inner, .. }) => other.is_compatible_with(inner),
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
            (Type::Enum(n1), Type::Instantiated { name: n2, .. }) | (Type::Instantiated { name: n1, .. }, Type::Enum(n2)) => {
                n1 == n2
            }
            (Type::Array { elem: e1, len: l1 }, Type::Array { elem: e2, len: l2 }) => {
                l1 == l2 && e1.is_compatible_with(e2)
            }
            (Type::Ref { is_mut: m1, inner: i1, region: r1 }, Type::Ref { is_mut: m2, inner: i2, region: r2 }) => {
                let region_compat = match (r1, r2) {
                    (Some(reg1), Some(reg2)) => reg1.0 <= reg2.0,
                    _ => true,
                };
                (*m1 == *m2 || (!m2 && *m1)) && i1.is_compatible_with(i2) && region_compat
            }
            (Type::Ptr { is_mut: m1, inner: i1 }, Type::Ptr { is_mut: m2, inner: i2 }) => {
                (!*m2 || *m1) && (i1.is_compatible_with(i2) || **i1 == Type::U8 || **i2 == Type::U8)
            }
            (Type::Ref { is_mut: m1, inner: i1, .. }, Type::Ptr { is_mut: m2, inner: i2 }) => {
                (!*m2 || *m1) && (i1.is_compatible_with(i2) || **i2 == Type::U8)
            }
            (Type::String, Type::Ptr { is_mut: false, inner }) if **inner == Type::U8 => true,
            (Type::Fn { params: p1, return_type: r1, yields_effects: e1 }, Type::Fn { params: p2, return_type: r2, yields_effects: e2 }) => {
                if p1.len() != p2.len() {
                    return false;
                }
                let params_compat = p1.iter().zip(p2.iter()).all(|(a, b)| b.is_compatible_with(a));
                let ret_compat = r1.is_compatible_with(r2);
                let s1: std::collections::BTreeSet<_> = e1.iter().map(|e| e.trim_start_matches("..")).collect();
                let s2: std::collections::BTreeSet<_> = e2.iter().map(|e| e.trim_start_matches("..")).collect();
                let effects_compat = s2.is_subset(&s1) || s1 == s2;
                params_compat && ret_compat && effects_compat
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
            Type::Ptr { is_mut, inner } => {
                let m_str = if *is_mut { "mut " } else { "" };
                write!(f, "*{}{}", m_str, inner)
            }

            Type::Struct(name) => write!(f, "{}", name),
            Type::Enum(name) => write!(f, "{}", name),
            Type::Array { elem, len } => write!(f, "[{}; {}]", elem, len),
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
            Type::RegionBounded { inner, region } => write!(f, "{} in {}", inner, region),
        }
    }
}
