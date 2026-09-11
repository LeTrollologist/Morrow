use crate::ir::*;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct TirVerifyError {
    pub message: String,
    pub function_name: String,
    pub block_id: Option<BlockId>,
}

impl std::fmt::Display for TirVerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(bid) = self.block_id {
            write!(f, "[TIR Error in fn '{}' at {}]: {}", self.function_name, bid, self.message)
        } else {
            write!(f, "[TIR Error in fn '{}']: {}", self.function_name, self.message)
        }
    }
}

pub fn verify_module(module: &TirModule) -> Result<(), Vec<TirVerifyError>> {
    let mut errors = Vec::new();
    for func in &module.functions {
        if let Err(mut func_errs) = verify_function(func) {
            errors.append(&mut func_errs);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn verify_function(func: &TirFunction) -> Result<(), Vec<TirVerifyError>> {
    let mut errors = Vec::new();
    let block_ids: HashSet<BlockId> = func.blocks.iter().map(|b| b.id).collect();

    if !block_ids.contains(&func.entry_block) {
        errors.push(TirVerifyError {
            message: format!("Entry block {} does not exist in function", func.entry_block),
            function_name: func.name.clone(),
            block_id: Some(func.entry_block),
        });
    }

    for block in &func.blocks {
        if block.terminator.is_none() {
            errors.push(TirVerifyError {
                message: format!("Block {} is missing a terminator", block.id),
                function_name: func.name.clone(),
                block_id: Some(block.id),
            });
            continue;
        }

        match block.terminator.as_ref().unwrap() {
            Terminator::Branch(target) => {
                if !block_ids.contains(target) {
                    errors.push(TirVerifyError {
                        message: format!("Branch target {} does not exist", target),
                        function_name: func.name.clone(),
                        block_id: Some(block.id),
                    });
                }
            }
            Terminator::BranchCond { then_block, else_block, .. } => {
                if !block_ids.contains(then_block) {
                    errors.push(TirVerifyError {
                        message: format!("BranchCond then target {} does not exist", then_block),
                        function_name: func.name.clone(),
                        block_id: Some(block.id),
                    });
                }
                if !block_ids.contains(else_block) {
                    errors.push(TirVerifyError {
                        message: format!("BranchCond else target {} does not exist", else_block),
                        function_name: func.name.clone(),
                        block_id: Some(block.id),
                    });
                }
            }
            Terminator::HandleEffect { body_entry, handlers, exit_block } => {
                if !block_ids.contains(body_entry) {
                    errors.push(TirVerifyError {
                        message: format!("HandleEffect body entry {} does not exist", body_entry),
                        function_name: func.name.clone(),
                        block_id: Some(block.id),
                    });
                }
                if !block_ids.contains(exit_block) {
                    errors.push(TirVerifyError {
                        message: format!("HandleEffect exit block {} does not exist", exit_block),
                        function_name: func.name.clone(),
                        block_id: Some(block.id),
                    });
                }
                for h in handlers {
                    if !block_ids.contains(&h.handler_entry) {
                        errors.push(TirVerifyError {
                            message: format!("Handler entry {} for {}::{} does not exist", h.handler_entry, h.effect, h.op),
                            function_name: func.name.clone(),
                            block_id: Some(block.id),
                        });
                    }
                }
            }
            Terminator::Resume { continuation_block, .. } => {
                if !block_ids.contains(continuation_block) {
                    errors.push(TirVerifyError {
                        message: format!("Resume target {} does not exist", continuation_block),
                        function_name: func.name.clone(),
                        block_id: Some(block.id),
                    });
                }
            }
            Terminator::Return(_) | Terminator::Unreachable => {}
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
