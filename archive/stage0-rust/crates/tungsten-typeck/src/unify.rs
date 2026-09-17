use std::collections::HashMap;
use crate::types::Type;

#[derive(Debug, Clone, Default)]
pub struct Subst {
    pub bindings: HashMap<String, Type>,
    pub effect_bindings: HashMap<String, Vec<String>>,
}

impl Subst {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            effect_bindings: HashMap::new(),
        }
    }

    pub fn bind(&mut self, param: String, ty: Type) {
        self.bindings.insert(param, ty);
    }

    pub fn get(&self, param: &str) -> Option<&Type> {
        self.bindings.get(param)
    }
}

pub fn unify(expected: &Type, actual: &Type, subst: &mut Subst) -> Result<(), String> {
    if expected == actual {
        return Ok(());
    }

    match (expected, actual) {
        (Type::GenericParam(name), ty) => {
            if let Some(existing) = subst.get(name) {
                if !ty.is_compatible_with(existing) {
                    return Err(format!("Cannot unify generic '{}': inferred as '{}' but found incompatible argument '{}'", name, existing, ty));
                }
            } else {
                subst.bind(name.clone(), ty.clone());
            }
            Ok(())
        }
        (Type::RegionBounded { inner, region: r1 }, Type::RegionBounded { inner: i2, region: r2 }) => {
            if r1.0 < r2.0 {
                return Err(format!("Region lifetime mismatch: expected region '{}', found inner region '{}'", r1, r2));
            }
            unify(inner, i2, subst)
        }
        (Type::RegionBounded { inner, .. }, other) => unify(inner, other, subst),
        (other, Type::RegionBounded { inner, .. }) => unify(other, inner, subst),
        (Type::Ptr { is_mut: m1, inner: i1 }, Type::Ptr { is_mut: m2, inner: i2 }) => {
            if *m1 && !*m2 {
                return Err("Cannot unify mutable pointer with immutable pointer".into());
            }
            if **i1 == Type::U8 || **i2 == Type::U8 {
                Ok(())
            } else {
                unify(i1, i2, subst)
            }
        }
        (Type::Instantiated { name: n1, args: a1 }, Type::Instantiated { name: n2, args: a2 }) => {
            if n1 != n2 || a1.len() != a2.len() {
                return Err(format!("Cannot unify '{}' with '{}'", expected, actual));
            }
            for (arg1, arg2) in a1.iter().zip(a2.iter()) {
                unify(arg1, arg2, subst)?;
            }
            Ok(())
        }
        (Type::Ref { is_mut: m1, inner: i1, .. }, Type::Ref { is_mut: m2, inner: i2, .. }) => {
            if *m1 && !*m2 {
                return Err("Cannot unify mutable reference with immutable reference".into());
            }
            unify(i1, i2, subst)
        }
        (Type::Fn { params: p1, return_type: r1, yields_effects: e1 }, Type::Fn { params: p2, return_type: r2, yields_effects: e2 }) => {
            if p1.len() != p2.len() {
                return Err("Function parameter count mismatch".into());
            }
            for (arg1, arg2) in p1.iter().zip(p2.iter()) {
                unify(arg1, arg2, subst)?;
            }

            let is_row_var = |s: &str| s.starts_with("..") || (s.len() == 1 && s.chars().next().map_or(false, |c| c.is_ascii_uppercase())) || s.starts_with('?');
            let clean = |s: &str| s.trim_start_matches("..").to_string();

            let mut row_var1 = None;
            let mut concrete1 = std::collections::BTreeSet::new();
            for eff in e1 {
                if is_row_var(eff) {
                    row_var1 = Some(clean(eff));
                } else {
                    concrete1.insert(eff.clone());
                }
            }

            let mut row_var2 = None;
            let mut concrete2 = std::collections::BTreeSet::new();
            for eff in e2 {
                if is_row_var(eff) {
                    row_var2 = Some(clean(eff));
                } else {
                    concrete2.insert(eff.clone());
                }
            }

            if let Some(rv1) = row_var1 {
                let diff: Vec<String> = concrete2.difference(&concrete1).cloned().collect();
                if let Some(existing) = subst.effect_bindings.get(&rv1) {
                    let s_exist: std::collections::BTreeSet<_> = existing.iter().cloned().collect();
                    let s_diff: std::collections::BTreeSet<_> = diff.into_iter().collect();
                    let union_set: Vec<String> = s_exist.union(&s_diff).cloned().collect();
                    subst.effect_bindings.insert(rv1.clone(), union_set.clone());
                    subst.effect_bindings.insert(format!("..{}", rv1), union_set);
                } else {
                    subst.effect_bindings.insert(rv1.clone(), diff.clone());
                    subst.effect_bindings.insert(format!("..{}", rv1), diff);
                }
            } else if let Some(rv2) = row_var2 {
                let diff: Vec<String> = concrete1.difference(&concrete2).cloned().collect();
                subst.effect_bindings.insert(rv2.clone(), diff.clone());
                subst.effect_bindings.insert(format!("..{}", rv2), diff);
            } else {
                for eff in &concrete1 {
                    if !concrete2.contains(eff) {
                        return Err(format!("Effect '{}' missing in function type unification", eff));
                    }
                }
            }

            unify(r1, r2, subst)
        }
        (other, Type::GenericParam(name)) => {
            subst.bind(name.clone(), other.clone());
            Ok(())
        }
        (t1, t2) if t1 == t2 => Ok(()),
        (t1, t2) => Err(format!("Type mismatch: cannot unify '{}' with '{}'", t1, t2)),
    }
}

pub fn substitute(ty: &Type, subst: &Subst) -> Type {
    match ty {
        Type::GenericParam(name) => {
            if let Some(replacement) = subst.get(name) {
                replacement.clone()
            } else {
                ty.clone()
            }
        }
        Type::Instantiated { name, args } => {
            let new_args = args.iter().map(|a| substitute(a, subst)).collect();
            Type::Instantiated {
                name: name.clone(),
                args: new_args,
            }
        }
        Type::Ref { is_mut, inner, region } => {
            Type::Ref {
                is_mut: *is_mut,
                inner: Box::new(substitute(inner, subst)),
                region: *region,
            }
        }
        Type::Fn { params, return_type, yields_effects } => {
            let new_params = params.iter().map(|p| substitute(p, subst)).collect();
            let new_ret = substitute(return_type, subst);
            let mut new_effects = std::collections::BTreeSet::new();
            for eff in yields_effects {
                let clean_name = eff.trim_start_matches("..");
                if let Some(bound_effs) = subst.effect_bindings.get(clean_name).or_else(|| subst.effect_bindings.get(eff)) {
                    for b in bound_effs {
                        new_effects.insert(b.clone());
                    }
                } else {
                    new_effects.insert(eff.clone());
                }
            }
            Type::Fn {
                params: new_params,
                return_type: Box::new(new_ret),
                yields_effects: new_effects.into_iter().collect(),
            }
        }
        Type::RegionBounded { inner, region } => {
            Type::RegionBounded {
                inner: Box::new(substitute(inner, subst)),
                region: *region,
            }
        }
        Type::Ptr { is_mut, inner } => {
            Type::Ptr {
                is_mut: *is_mut,
                inner: Box::new(substitute(inner, subst)),
            }
        }
        Type::Array { elem, len } => {
            Type::Array {
                elem: Box::new(substitute(elem, subst)),
                len: *len,
            }
        }
        other => other.clone(),
    }
}
