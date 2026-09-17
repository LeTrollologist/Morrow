use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectRow {
    pub effects: HashSet<String>,
}

impl EffectRow {
    pub fn new() -> Self {
        Self {
            effects: HashSet::new(),
        }
    }

    pub fn from_slice(effects: &[String]) -> Self {
        let mut row = Self::new();
        for eff in effects {
            row.effects.insert(eff.clone());
        }
        row
    }

    pub fn insert(&mut self, effect: String) {
        self.effects.insert(effect);
    }

    pub fn remove(&mut self, effect: &str) -> bool {
        self.effects.remove(effect)
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    pub fn union_with(&mut self, other: &EffectRow) {
        for eff in &other.effects {
            self.effects.insert(eff.clone());
        }
    }

    pub fn diff(&self, handled: &HashSet<String>) -> EffectRow {
        let mut remaining = HashSet::new();
        for eff in &self.effects {
            if !handled.contains(eff) {
                remaining.insert(eff.clone());
            }
        }
        EffectRow { effects: remaining }
    }
}
