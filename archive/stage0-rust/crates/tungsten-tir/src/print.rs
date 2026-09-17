use crate::ir::*;
use std::fmt::Write;

pub fn print_module(module: &TirModule) -> String {
    let mut out = String::new();
    writeln!(out, ";; Tungsten Intermediate Representation (TIR)").unwrap();
    writeln!(out, ";; Target: SSA Basic-Block CFG").unwrap();
    writeln!(out).unwrap();

    for func in &module.functions {
        writeln!(out, "{}", print_function(func)).unwrap();
    }

    out
}

pub fn print_function(func: &TirFunction) -> String {
    let mut out = String::new();

    let type_params_str = if func.type_params.is_empty() {
        "".to_string()
    } else {
        format!("<{}>", func.type_params.join(", "))
    };

    let params_str = func
        .params
        .iter()
        .map(|p| {
            let mut s = format!("{}: {}", p.name, p.ty);
            if let Some(int) = p.interval {
                s.push_str(&format!(" [{}, {}]", int.min, int.max));
            }
            s
        })
        .collect::<Vec<_>>()
        .join(", ");

    let yields_str = if func.yields_effects.is_empty() {
        "".to_string()
    } else {
        format!(" yields [{}]", func.yields_effects.join(", "))
    };

    writeln!(
        out,
        "fn {}{}({}) -> {}{} {{",
        func.name, type_params_str, params_str, func.return_type, yields_str
    )
    .unwrap();

    for block in &func.blocks {
        if let Some(lbl) = &block.label {
            writeln!(out, "  {}: ; label: {}", block.id, lbl).unwrap();
        } else {
            writeln!(out, "  {}:", block.id).unwrap();
        }

        for inst in &block.instructions {
            match inst {
                Instruction::Assign { dest, rvalue, ty, .. } => {
                    writeln!(out, "    {} : {} = {}", dest, ty, print_rvalue(rvalue)).unwrap();
                }
                Instruction::AssertRefinement { operand, interval, error_msg, .. } => {
                    writeln!(
                        out,
                        "    assert_refinement {} within [{}, {}] ; \"{}\"",
                        operand, interval.min, interval.max, error_msg
                    )
                    .unwrap();
                }
                Instruction::PerformEffect { effect, op, args, dest, ty, .. } => {
                    let args_str = args.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ");
                    if let Some(d) = dest {
                        writeln!(out, "    {} : {} = perform {}::{}({})", d, ty, effect, op, args_str).unwrap();
                    } else {
                        writeln!(out, "    perform {}::{}({})", effect, op, args_str).unwrap();
                    }
                }
                Instruction::Call { dest, func, args, ty, .. } => {
                    let args_str = args.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ");
                    if let Some(d) = dest {
                        writeln!(out, "    {} : {} = call {}({})", d, ty, func, args_str).unwrap();
                    } else {
                        writeln!(out, "    call {}({})", func, args_str).unwrap();
                    }
                }
                Instruction::ExternCall { dest, func, args, ty, .. } => {
                    let args_str = args.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ");
                    if let Some(d) = dest {
                        writeln!(out, "    {} : {} = extern_call {}({})", d, ty, func, args_str).unwrap();
                    } else {
                        writeln!(out, "    extern_call {}({})", func, args_str).unwrap();
                    }
                }
                Instruction::Store { ptr, value, .. } => {
                    writeln!(out, "    store {} = {}", ptr, value).unwrap();
                }
                Instruction::StoreIndex { target, index, stride, value, .. } => {
                    writeln!(out, "    store_index {}[{}] (stride {}) = {}", target, index, stride, value).unwrap();
                }
                Instruction::SetField { base, field, val, .. } => {
                    writeln!(out, "    set_field {}.{} = {}", base, field, val).unwrap();
                }
                Instruction::RegionEnter { dest, region_id, .. } => {
                    writeln!(out, "    {} = region_enter(r{})", dest, region_id).unwrap();
                }
                Instruction::RegionExit { arena, .. } => {
                    writeln!(out, "    region_exit({})", arena).unwrap();
                }
                Instruction::NurseryEnter { dest, nursery_id, .. } => {
                    writeln!(out, "    {} = nursery_enter(n{})", dest, nursery_id).unwrap();
                }
                Instruction::NurseryExit { nursery, .. } => {
                    writeln!(out, "    nursery_exit({})", nursery).unwrap();
                }
            }
        }

        if let Some(term) = &block.terminator {
            match term {
                Terminator::Return(Some(op)) => {
                    writeln!(out, "    return {}", op).unwrap();
                }
                Terminator::Return(None) => {
                    writeln!(out, "    return ()").unwrap();
                }
                Terminator::Branch(target) => {
                    writeln!(out, "    branch {}", target).unwrap();
                }
                Terminator::BranchCond { cond, then_block, else_block } => {
                    writeln!(out, "    branch_if {}, then: {}, else: {}", cond, then_block, else_block).unwrap();
                }
                Terminator::HandleEffect { body_entry, handlers, exit_block } => {
                    let h_strs = handlers
                        .iter()
                        .map(|h| format!("{}::{}({:?}) => {}", h.effect, h.op, h.param, h.handler_entry))
                        .collect::<Vec<_>>()
                        .join(", ");
                    writeln!(out, "    handle_effect body: {}, handlers: [{}], exit: {}", body_entry, h_strs, exit_block).unwrap();
                }
                Terminator::Resume { arg, continuation_block } => {
                    match arg {
                        Some(a) => writeln!(out, "    resume {} -> {}", a, continuation_block).unwrap(),
                        None => writeln!(out, "    resume -> {}", continuation_block).unwrap(),
                    }
                }
                Terminator::Unreachable => {
                    writeln!(out, "    unreachable").unwrap();
                }
            }
        } else {
            writeln!(out, "    <missing terminator>").unwrap();
        }
    }

    writeln!(out, "}}").unwrap();
    out
}

fn print_rvalue(rv: &RValue) -> String {
    match rv {
        RValue::Use(op) => format!("use {}", op),
        RValue::BinaryOp(op, l, r) => format!("{:?} {}, {}", op, l, r),
        RValue::FieldAccess { target, field } => format!("{}.{}", target, field),
        RValue::MethodCall { target, method, args } => {
            let args_str = args.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ");
            format!("{}.{}({})", target, method, args_str)
        }
        RValue::StructInit { name, fields, arena } => {
            let f_strs = fields.iter().map(|(k, v)| format!("{}: {}", k, v)).collect::<Vec<_>>().join(", ");
            if let Some(a) = arena {
                format!("{} in {} {{ {} }}", name, a, f_strs)
            } else {
                format!("{} {{ {} }}", name, f_strs)
            }
        }
        RValue::Ref { is_mut, operand } => {
            if *is_mut {
                format!("&mut {}", operand)
            } else {
                format!("&{}", operand)
            }
        }
        RValue::Cast { operand, target_ty } => {
            format!("cast {} as {}", operand, target_ty)
        }
        RValue::EnumInit { enum_name, variant, tag, payload, arena } => {
            let p_strs = payload.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ");
            if let Some(a) = arena {
                format!("{}::{}#{} in {} ({})", enum_name, variant, tag, a, p_strs)
            } else {
                format!("{}::{}#{} ({})", enum_name, variant, tag, p_strs)
            }
        }
        RValue::EnumTag(op) => format!("enum_tag({})", op),
        RValue::EnumPayload { target, index } => format!("enum_payload({}, {})", target, index),
        RValue::ArrayInit { elements, elem_stride, arena } => {
            let e_strs = elements.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ");
            if let Some(a) = arena {
                format!("[{}] (stride {}) in {}", e_strs, elem_stride, a)
            } else {
                format!("[{}] (stride {})", e_strs, elem_stride)
            }
        }
        RValue::ArrayIndex { target, index, stride } => {
            format!("{}[{} * {}]", target, index, stride)
        }
        RValue::Deref(op) => format!("*{}", op),
        RValue::AddrOf(op) => format!("&raw {}", op),
    }
}
