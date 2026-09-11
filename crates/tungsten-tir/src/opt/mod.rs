pub mod bounds_elim;
pub mod const_fold;
pub mod dce;

use crate::ir::TirModule;

#[derive(Debug, Default, Clone)]
pub struct OptStats {
    pub const_folds: usize,
    pub bounds_checks_eliminated: usize,
    pub dead_code_pruned: usize,
    pub iterations: usize,
}

pub struct PassManager {
    max_iterations: usize,
}

impl PassManager {
    pub fn new() -> Self {
        Self { max_iterations: 5 }
    }

    pub fn run(&self, module: &mut TirModule) -> OptStats {
        let mut total_stats = OptStats::default();

        for func in &mut module.functions {
            for iter in 0..self.max_iterations {
                total_stats.iterations = total_stats.iterations.max(iter + 1);
                let mut changes = 0;

                let cf = const_fold::run_const_fold(func);
                total_stats.const_folds += cf;
                changes += cf;

                let be = bounds_elim::run_bounds_elim(func);
                total_stats.bounds_checks_eliminated += be;
                changes += be;

                let dce = dce::run_dce(func);
                total_stats.dead_code_pruned += dce;
                changes += dce;

                if changes == 0 {
                    break;
                }
            }
        }

        total_stats
    }
}
