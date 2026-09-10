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
                if !existing.is_compatible_with(ty) && !ty.is_compatible_with(existing) {
                    return Err(format!("Cannot unify generic '{}': inferred as '{}' and '{}'", name, existing, ty));
                }
            } else {
                subst.bind(name.clone(), ty.clone());
            }
            Ok(())
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
        (Type::Ref { is_mut: m1, inner: i1 }, Type::Ref { is_mut: m2, inner: i2 }) => {
            if *m1 && !*m2 {
                return Err("Cannot unify mutable reference with immutable reference".into());
            }
            unify(i1, i2, subst)
        }
        (Type::Fn { params: p1, return_type: r1, yields_effects: e1 }, Type::Fn { params: p2, return_type: r2, yields_effects: e2 }) => {
            if p1.len() != p2.len() {
                return Err("Function parameter count mismatch".into());
            }
            for (param1, param2) in p1.iter().zip(p2.iter()) {
                unify(param1, param2, subst)?;
            }
            unify(r1, r2, subst)?;

            // Bind effect parameters if present
            for eff in e1 {
                if !eff.is_empty() && eff.chars().next().unwrap().is_ascii_uppercase() {
                    subst.effect_bindings.insert(eff.clone(), e2.clone());
                }
            }
            Ok(())
        }
        (e, a) if e.is_compatible_with(a) => Ok(()),
        _ => Err(format!("Type mismatch during unification: expected '{}', found '{}'", expected, actual)),
    }
}

pub fn substitute(ty: &Type, subst: &Subst) -> Type {
    match ty {
        Type::GenericParam(name) => {
            if let Some(bound) = subst.get(name) {
                bound.clone()
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
        Type::Ref { is_mut, inner } => {
            Type::Ref {
                is_mut: *is_mut,
                inner: Box::new(substitute(inner, subst)),
            }
        }
        Type::Fn { params, return_type, yields_effects } => {
            let new_params = params.iter().map(|p| substitute(p, subst)).collect();
            let new_ret = substitute(return_type, subst);
            let mut new_effects = Vec::new();
            for eff in yields_effects {
                if let Some(bound_effs) = subst.effect_bindings.get(eff) {
                    new_effects.extend(bound_effs.clone());
                } else {
                    new_effects.push(eff.clone());
                }
            }
            Type::Fn {
                params: new_params,
                return_type: Box::new(new_ret),
                yields_effects: new_effects,
            }
        }
        other => other.clone(),
    }
}
