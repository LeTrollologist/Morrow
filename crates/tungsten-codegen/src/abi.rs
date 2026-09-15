use cranelift::prelude::*;
use tungsten_typeck::types::Type;

pub fn to_cranelift_type(ty: &Type, ptr_type: types::Type) -> Option<types::Type> {
    match ty {
        Type::U8 => Some(types::I8),
        Type::U16 => Some(types::I16),
        Type::U32 => Some(types::I32),
        Type::U64 | Type::I64 | Type::Usize => Some(types::I64),
        Type::Bool => Some(types::I8),
        Type::Refined { base, .. } => to_cranelift_type(base, ptr_type),
        Type::Relational { base, .. } => to_cranelift_type(base, ptr_type),
        Type::String | Type::Ref { .. } | Type::Ptr { .. } | Type::Struct(_) | Type::Instantiated { .. } | Type::Enum(_) | Type::Array { .. } => {
            Some(ptr_type)
        }
        Type::Fn { .. } => Some(ptr_type),
        Type::Unit => None,
        Type::GenericParam(_) => Some(types::I64),
    }
}
