use crate::ir::*;
use std::collections::HashMap;
use tungsten_syntax::ast::BinOp;

pub fn run_const_fold(func: &mut TirFunction) -> usize {
    let mut changes = 0;

    for block in &mut func.blocks {
        let mut const_map: HashMap<Var, TirConstant> = HashMap::new();
        for inst in &mut block.instructions {
            if let Instruction::Assign { dest, rvalue, .. } = inst {
                // 1. Substitute operands from const_map
                substitute_rvalue_operands(rvalue, &const_map);

                // 2. Fold binary ops on constants
                if let RValue::BinaryOp(op, Operand::Constant(l), Operand::Constant(r)) = rvalue {
                    if let Some(folded) = fold_binop(*op, l, r) {
                        *rvalue = RValue::Use(Operand::Constant(folded.clone()));
                        const_map.insert(dest.clone(), folded);
                        changes += 1;
                        continue;
                    }
                }

                // 3. Track direct constant uses
                if let RValue::Use(Operand::Constant(c)) = rvalue {
                    const_map.insert(dest.clone(), c.clone());
                }
            }
        }

        // Fold conditional branches on known constants
        if let Some(Terminator::BranchCond { cond, then_block, else_block }) = &block.terminator {
            let actual_cond = match cond {
                Operand::Constant(c) => Some(c.clone()),
                Operand::Var(v, _) => const_map.get(v).cloned(),
            };

            if let Some(TirConstant::Bool(b)) = actual_cond {
                if b {
                    block.terminator = Some(Terminator::Branch(*then_block));
                } else {
                    block.terminator = Some(Terminator::Branch(*else_block));
                }
                changes += 1;
            }
        }
    }

    changes
}

fn substitute_rvalue_operands(rv: &mut RValue, const_map: &HashMap<Var, TirConstant>) {
    match rv {
        RValue::Use(op) => substitute_operand(op, const_map),
        RValue::BinaryOp(_, l, r) => {
            substitute_operand(l, const_map);
            substitute_operand(r, const_map);
        }
        RValue::FieldAccess { target, .. } => substitute_operand(target, const_map),
        RValue::MethodCall { target, args, .. } => {
            substitute_operand(target, const_map);
            for a in args {
                substitute_operand(a, const_map);
            }
        }
        RValue::StructInit { fields, .. } => {
            for (_, op) in fields {
                substitute_operand(op, const_map);
            }
        }
        RValue::Ref { operand, .. } => substitute_operand(operand, const_map),
        RValue::Cast { operand, .. } => substitute_operand(operand, const_map),
        RValue::EnumInit { payload, .. } => {
            for op in payload {
                substitute_operand(op, const_map);
            }
        }
        RValue::EnumTag(op) => substitute_operand(op, const_map),
        RValue::EnumPayload { target, .. } => substitute_operand(target, const_map),
        RValue::ArrayInit { elements, .. } => {
            for op in elements {
                substitute_operand(op, const_map);
            }
        }
        RValue::ArrayIndex { target, index, .. } => {
            substitute_operand(target, const_map);
            substitute_operand(index, const_map);
        }
        RValue::Deref(op) => substitute_operand(op, const_map),
        RValue::AddrOf(op) => substitute_operand(op, const_map),
    }
}

fn substitute_operand(op: &mut Operand, const_map: &HashMap<Var, TirConstant>) {
    if let Operand::Var(v, _) = op {
        if let Some(c) = const_map.get(v) {
            *op = Operand::Constant(c.clone());
        }
    }
}

fn fold_binop(op: BinOp, left: &TirConstant, right: &TirConstant) -> Option<TirConstant> {
    match (left, right) {
        (TirConstant::Int(l), TirConstant::Int(r)) => match op {
            BinOp::Add => Some(TirConstant::Int(l.wrapping_add(*r))),
            BinOp::Sub => Some(TirConstant::Int(l.wrapping_sub(*r))),
            BinOp::Mul => Some(TirConstant::Int(l.wrapping_mul(*r))),
            BinOp::Div => {
                if *r != 0 {
                    Some(TirConstant::Int(l / r))
                } else {
                    None
                }
            }
            BinOp::Eq => Some(TirConstant::Bool(l == r)),
            BinOp::NotEq => Some(TirConstant::Bool(l != r)),
            BinOp::Lt => Some(TirConstant::Bool(l < r)),
            BinOp::LtEq => Some(TirConstant::Bool(l <= r)),
            BinOp::Gt => Some(TirConstant::Bool(l > r)),
            BinOp::GtEq => Some(TirConstant::Bool(l >= r)),
            _ => None,
        },
        (TirConstant::Bool(l), TirConstant::Bool(r)) => match op {
            BinOp::And => Some(TirConstant::Bool(*l && *r)),
            BinOp::Or => Some(TirConstant::Bool(*l || *r)),
            BinOp::Eq => Some(TirConstant::Bool(l == r)),
            BinOp::NotEq => Some(TirConstant::Bool(l != r)),
            _ => None,
        },
        _ => None,
    }
}
