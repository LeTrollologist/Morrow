use crate::ir::*;
use std::collections::HashSet;

pub fn run_dce(func: &mut TirFunction) -> usize {
    let mut pruned = 0;

    // 1. Unreachable block elimination
    let mut reachable = HashSet::new();
    let mut worklist = vec![func.entry_block];
    reachable.insert(func.entry_block);

    while let Some(curr) = worklist.pop() {
        if let Some(block) = func.blocks.iter().find(|b| b.id == curr) {
            if let Some(term) = &block.terminator {
                match term {
                    Terminator::Branch(target) => {
                        if reachable.insert(*target) {
                            worklist.push(*target);
                        }
                    }
                    Terminator::BranchCond { then_block, else_block, .. } => {
                        if reachable.insert(*then_block) {
                            worklist.push(*then_block);
                        }
                        if reachable.insert(*else_block) {
                            worklist.push(*else_block);
                        }
                    }
                    Terminator::HandleEffect { body_entry, handlers, exit_block } => {
                        if reachable.insert(*body_entry) {
                            worklist.push(*body_entry);
                        }
                        if reachable.insert(*exit_block) {
                            worklist.push(*exit_block);
                        }
                        for h in handlers {
                            if reachable.insert(h.handler_entry) {
                                worklist.push(h.handler_entry);
                            }
                        }
                    }
                    Terminator::Resume { continuation_block, .. } => {
                        if reachable.insert(*continuation_block) {
                            worklist.push(*continuation_block);
                        }
                    }
                    Terminator::Return(_) | Terminator::Unreachable => {}
                }
            }
        }
    }

    let initial_block_count = func.blocks.len();
    func.blocks.retain(|b| reachable.contains(&b.id));
    pruned += initial_block_count - func.blocks.len();

    // 2. Dead temporary assignment elimination
    let mut used_vars = HashSet::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            collect_used_vars_in_inst(inst, &mut used_vars);
        }
        if let Some(term) = &block.terminator {
            collect_used_vars_in_term(term, &mut used_vars);
        }
    }

    for block in &mut func.blocks {
        let initial_inst_count = block.instructions.len();
        block.instructions.retain(|inst| {
            if let Instruction::Assign { dest: Var::Temp(id), rvalue, .. } = inst {
                if !used_vars.contains(&Var::Temp(*id)) && is_pure_rvalue(rvalue) {
                    return false;
                }
            }
            true
        });
        pruned += initial_inst_count - block.instructions.len();
    }

    pruned
}

fn is_pure_rvalue(rv: &RValue) -> bool {
    match rv {
        RValue::Use(_) | RValue::BinaryOp(_, _, _) | RValue::Ref { .. } | RValue::Cast { .. } => true,
        _ => false,
    }
}

fn collect_used_vars_in_inst(inst: &Instruction, used: &mut HashSet<Var>) {
    match inst {
        Instruction::Assign { rvalue, .. } => collect_used_vars_in_rvalue(rvalue, used),
        Instruction::AssertRefinement { operand, .. } => collect_used_vars_in_operand(operand, used),
        Instruction::PerformEffect { args, .. } => {
            for a in args {
                collect_used_vars_in_operand(a, used);
            }
        }
        Instruction::Call { func, args, .. } => {
            collect_used_vars_in_operand(func, used);
            for a in args {
                collect_used_vars_in_operand(a, used);
            }
        }
        Instruction::SetField { base, val, .. } => {
            used.insert(base.clone());
            collect_used_vars_in_operand(val, used);
        }
    }
}

fn collect_used_vars_in_rvalue(rv: &RValue, used: &mut HashSet<Var>) {
    match rv {
        RValue::Use(op) => collect_used_vars_in_operand(op, used),
        RValue::BinaryOp(_, l, r) => {
            collect_used_vars_in_operand(l, used);
            collect_used_vars_in_operand(r, used);
        }
        RValue::FieldAccess { target, .. } => collect_used_vars_in_operand(target, used),
        RValue::MethodCall { target, args, .. } => {
            collect_used_vars_in_operand(target, used);
            for a in args {
                collect_used_vars_in_operand(a, used);
            }
        }
        RValue::StructInit { fields, .. } => {
            for (_, op) in fields {
                collect_used_vars_in_operand(op, used);
            }
        }
        RValue::Ref { operand, .. } => collect_used_vars_in_operand(operand, used),
        RValue::Cast { operand, .. } => collect_used_vars_in_operand(operand, used),
    }
}

fn collect_used_vars_in_operand(op: &Operand, used: &mut HashSet<Var>) {
    if let Operand::Var(v, _) = op {
        used.insert(v.clone());
    }
}

fn collect_used_vars_in_term(term: &Terminator, used: &mut HashSet<Var>) {
    match term {
        Terminator::Return(Some(op)) => collect_used_vars_in_operand(op, used),
        Terminator::BranchCond { cond, .. } => collect_used_vars_in_operand(cond, used),
        Terminator::Resume { arg: Some(op), .. } => collect_used_vars_in_operand(op, used),
        _ => {}
    }
}
