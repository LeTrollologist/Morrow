use crate::ir::*;
use std::collections::HashMap;
use tungsten_typeck::interval::Interval;
use tungsten_typeck::types::Type;

pub fn run_bounds_elim(func: &mut TirFunction) -> usize {
    let mut eliminated = 0;
    let mut known_intervals: HashMap<Var, Interval> = HashMap::new();

    // Seed intervals from parameters
    for p in &func.params {
        if let Some(int) = p.interval {
            known_intervals.insert(Var::Named(p.name.clone()), int);
        }
    }

    for block in &mut func.blocks {
        let initial_count = block.instructions.len();

        block.instructions.retain(|inst| {
            match inst {
                Instruction::Assign { dest, rvalue, ty, .. } => {
                    // If assigning a constant integer, track its interval
                    if let RValue::Use(Operand::Constant(TirConstant::Int(n))) = rvalue {
                        known_intervals.insert(dest.clone(), Interval::point(*n));
                    } else if let Type::Refined { interval, .. } = ty {
                        known_intervals.insert(dest.clone(), *interval);
                    } else if let Some(int) = ty.default_interval() {
                        known_intervals.insert(dest.clone(), int);
                    }
                    true
                }
                Instruction::AssertRefinement { operand, interval, .. } => {
                    // Check if statically proven safe
                    match operand {
                        Operand::Constant(TirConstant::Int(n)) => {
                            if *n >= interval.min && *n <= interval.max {
                                // Statically proven in bounds! Eliminate check.
                                return false;
                            }
                        }
                        Operand::Var(v, _) => {
                            if let Some(known) = known_intervals.get(v) {
                                if known.min >= interval.min && known.max <= interval.max {
                                    // Known interval is completely inside target interval! Eliminate check.
                                    return false;
                                }
                            }
                        }
                        _ => {}
                    }
                    true
                }
                _ => true,
            }
        });

        eliminated += initial_count - block.instructions.len();
    }

    eliminated
}
