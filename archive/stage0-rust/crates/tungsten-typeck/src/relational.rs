use std::collections::HashMap;
use tungsten_syntax::ast::{BinOp, Expr, ExprKind};
use crate::interval::Interval;

#[derive(Debug, Clone, PartialEq)]
pub enum Constraint {
    GtEq(String, String), // a >= b
    Gt(String, String),   // a > b
    LtEq(String, String), // a <= b
    Lt(String, String),   // a < b
    Eq(String, String),   // a == b
}

#[derive(Debug, Clone, Default)]
pub struct RelationalContext {
    // Known relationships between named variables
    constraints: Vec<Constraint>,
    // Constant interval bounds for variables
    variable_intervals: HashMap<String, Interval>,
}

impl RelationalContext {
    pub fn new() -> Self {
        Self {
            constraints: Vec::new(),
            variable_intervals: HashMap::new(),
        }
    }

    pub fn add_interval(&mut self, var: String, interval: Interval) {
        self.variable_intervals.insert(var, interval);
    }

    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    // Verify if a relational predicate holds for a given candidate value expression
    pub fn verify_predicate(&self, candidate: &Expr, predicate: &Expr) -> Result<(), String> {
        match &predicate.kind {
            ExprKind::Binary { op: BinOp::And, left, right } => {
                self.verify_predicate(candidate, left)?;
                self.verify_predicate(candidate, right)?;
                Ok(())
            }
            ExprKind::Binary { op, left, right } => {
                self.verify_atomic_relation(candidate, *op, left, right)
            }
            _ => Ok(()),
        }
    }

    fn verify_atomic_relation(&self, candidate: &Expr, op: BinOp, left: &Expr, right: &Expr) -> Result<(), String> {
        // The left operand in the predicate is often `_val` (the implicit candidate)
        let is_left_candidate = matches!(&left.kind, ExprKind::Ident(id) if id == "_val");
        let is_right_candidate = matches!(&right.kind, ExprKind::Ident(id) if id == "_val");

        let (target_val, bound_expr, relation_op) = if is_left_candidate {
            (candidate, right, op)
        } else if is_right_candidate {
            let inverted_op = match op {
                BinOp::Lt => BinOp::Gt,
                BinOp::LtEq => BinOp::GtEq,
                BinOp::Gt => BinOp::Lt,
                BinOp::GtEq => BinOp::LtEq,
                other => other,
            };
            (candidate, left, inverted_op)
        } else {
            (left, right, op)
        };

        // 1. If both candidate and bound are constant integers
        if let (ExprKind::Int(c_val), ExprKind::Int(b_val)) = (&target_val.kind, &bound_expr.kind) {
            let satisfied = match relation_op {
                BinOp::GtEq => c_val >= b_val,
                BinOp::Gt => c_val > b_val,
                BinOp::LtEq => c_val <= b_val,
                BinOp::Lt => c_val < b_val,
                BinOp::Eq => c_val == b_val,
                BinOp::NotEq => c_val != b_val,
                _ => true,
            };
            if satisfied {
                return Ok(());
            } else {
                return Err(format!("Constant value {} does not satisfy relational bound {:?}", c_val, relation_op));
            }
        }

        // 2. If bound is a variable name, check recorded linear constraints and intervals
        if let (ExprKind::Ident(c_name), ExprKind::Ident(b_name)) = (&target_val.kind, &bound_expr.kind) {
            if c_name == b_name {
                if matches!(relation_op, BinOp::GtEq | BinOp::LtEq | BinOp::Eq) {
                    return Ok(());
                }
            }

            // Check direct constraint store
            for c in &self.constraints {
                match (c, relation_op) {
                    (Constraint::GtEq(x, y), BinOp::GtEq) if x == c_name && y == b_name => return Ok(()),
                    (Constraint::Gt(x, y), BinOp::Gt | BinOp::GtEq) if x == c_name && y == b_name => return Ok(()),
                    (Constraint::LtEq(x, y), BinOp::LtEq) if x == c_name && y == b_name => return Ok(()),
                    (Constraint::Lt(x, y), BinOp::Lt | BinOp::LtEq) if x == c_name && y == b_name => return Ok(()),
                    (Constraint::Eq(x, y), BinOp::GtEq | BinOp::LtEq | BinOp::Eq) if (x == c_name && y == b_name) || (x == b_name && y == c_name) => return Ok(()),
                    _ => {}
                }
            }

            // Check intervals if available
            if let (Some(c_int), Some(b_int)) = (self.variable_intervals.get(c_name), self.variable_intervals.get(b_name)) {
                match relation_op {
                    BinOp::GtEq if c_int.min >= b_int.max => return Ok(()),
                    BinOp::Gt if c_int.min > b_int.max => return Ok(()),
                    BinOp::LtEq if c_int.max <= b_int.min => return Ok(()),
                    BinOp::Lt if c_int.max < b_int.min => return Ok(()),
                    _ => {}
                }
            }
        }

        // 3. Constant candidate vs variable bound (with known interval)
        if let (ExprKind::Int(c_val), ExprKind::Ident(b_name)) = (&target_val.kind, &bound_expr.kind) {
            if let Some(b_int) = self.variable_intervals.get(b_name) {
                let satisfied = match relation_op {
                    BinOp::GtEq => *c_val >= b_int.max,
                    BinOp::Gt => *c_val > b_int.max,
                    BinOp::LtEq => *c_val <= b_int.min,
                    BinOp::Lt => *c_val < b_int.min,
                    BinOp::Eq => b_int.min == b_int.max && *c_val == b_int.min,
                    _ => true,
                };
                if satisfied {
                    return Ok(());
                } else {
                    return Err(format!("Value {} violates relational bound {:?} against parameter '{}'", c_val, relation_op, b_name));
                }
            }
        }

        // 4. Fallback: accept if candidate cannot be disproven at this stage
        Ok(())
    }
}
