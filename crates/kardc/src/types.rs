//! The type system.
//!
//! v1 (the Gen-2 reboot) ships the procedural core: fixed-width integers, a
//! boolean and `void`. Optionals (`?T`), error unions (`!T`), structs, enums,
//! slices and pointers arrive in later roadmap versions — each one explicit,
//! with no hidden conversions, per Zig's philosophy.

/// A resolved type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Usize,
    Bool,
    Void,
}

impl Type {
    /// Resolve a source type name (an ordinary identifier) to a builtin type.
    pub fn from_name(s: &str) -> Option<Type> {
        Some(match s {
            "i8" => Type::I8,
            "i16" => Type::I16,
            "i32" => Type::I32,
            "i64" => Type::I64,
            "u8" => Type::U8,
            "u16" => Type::U16,
            "u32" => Type::U32,
            "u64" => Type::U64,
            "usize" => Type::Usize,
            "bool" => Type::Bool,
            "void" => Type::Void,
            _ => return None,
        })
    }

    /// The source spelling of this type.
    pub fn name(self) -> &'static str {
        match self {
            Type::I8 => "i8",
            Type::I16 => "i16",
            Type::I32 => "i32",
            Type::I64 => "i64",
            Type::U8 => "u8",
            Type::U16 => "u16",
            Type::U32 => "u32",
            Type::U64 => "u64",
            Type::Usize => "usize",
            Type::Bool => "bool",
            Type::Void => "void",
        }
    }

    /// The C type used to represent this type in the emitted backend code.
    pub fn c_name(self) -> &'static str {
        match self {
            Type::I8 => "int8_t",
            Type::I16 => "int16_t",
            Type::I32 => "int32_t",
            Type::I64 => "int64_t",
            Type::U8 => "uint8_t",
            Type::U16 => "uint16_t",
            Type::U32 => "uint32_t",
            Type::U64 => "uint64_t",
            Type::Usize => "uintptr_t",
            Type::Bool => "bool",
            Type::Void => "void",
        }
    }

    pub fn is_int(self) -> bool {
        matches!(
            self,
            Type::I8
                | Type::I16
                | Type::I32
                | Type::I64
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
                | Type::Usize
        )
    }

    pub fn is_signed(self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64)
    }
}
