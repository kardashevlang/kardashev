//! The type system.
//!
//! v0.111 shipped the procedural core: fixed-width integers, a boolean and
//! `void`. v0.112 adds **structs** (`Type::Struct(id)` + the [`StructTable`]).
//! Optionals (`?T`), error unions (`!T`), enums, slices and pointers arrive in
//! later roadmap versions — each one explicit, per Zig's philosophy.

use std::collections::HashMap;

/// A resolved type. A `Struct(id)` indexes the [`StructTable`] produced by
/// semantic analysis; the enum stays `Copy` and two struct types are equal iff
/// they share an id.
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
    Struct(u32),
    /// `?T` — an optional. The `u32` indexes the optional-inner table in
    /// [`StructTable`] (`optional_inner`). v0.114: inner is never itself
    /// optional (no `??T`).
    Optional(u32),
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
            // Struct / optional names are dynamic; sema formats them via the
            // StructTable.
            Type::Struct(_) => "struct",
            Type::Optional(_) => "optional",
        }
    }

    /// The C type used to represent this type in the emitted backend code.
    ///
    /// Defined for primitives only: a `Struct` type's C name depends on the
    /// [`StructTable`], so emit resolves it via `StructTable::c_name` and must
    /// never call this on a `Struct`.
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
            Type::Struct(_) => unreachable!("c_name on a struct type; use StructTable::c_name"),
            Type::Optional(_) => {
                unreachable!("c_name on an optional type; use StructTable::optional_c_name")
            }
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

/// A resolved struct definition: its source name and its ordered fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructInfo {
    pub name: String,
    /// Fields in declaration order, each `(field_name, field_type)`.
    pub fields: Vec<(String, Type)>,
}

impl StructInfo {
    /// The type of field `name`, if present.
    pub fn field_type(&self, name: &str) -> Option<Type> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, t)| *t)
    }
}

/// The table of all struct types in a program, built by semantic analysis and
/// consumed by the backend. Ids are dense indices assigned in declaration
/// order, so iterating `0..len()` yields structs in source order — exactly the
/// order the backend must emit their C typedefs.
#[derive(Clone, Debug, Default)]
pub struct StructTable {
    defs: Vec<StructInfo>,
    by_name: HashMap<String, u32>,
    /// Inner types of `?T` optionals, indexed by the id in `Type::Optional(id)`.
    /// Despite the name, this table holds the program's *composite* types too,
    /// not only structs.
    optional_inners: Vec<Type>,
}

impl StructTable {
    pub fn new() -> StructTable {
        StructTable::default()
    }

    /// Register a struct `name`, returning its id. If already interned, returns
    /// the existing id (fields are filled later via [`set_fields`]).
    pub fn intern(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.by_name.get(name) {
            return id;
        }
        let id = self.defs.len() as u32;
        self.defs.push(StructInfo {
            name: name.to_string(),
            fields: Vec::new(),
        });
        self.by_name.insert(name.to_string(), id);
        id
    }

    /// The id of struct `name`, if registered.
    pub fn id_of(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }

    pub fn get(&self, id: u32) -> &StructInfo {
        &self.defs[id as usize]
    }

    /// Replace the fields of an already-interned struct.
    pub fn set_fields(&mut self, id: u32, fields: Vec<(String, Type)>) {
        self.defs[id as usize].fields = fields;
    }

    /// The C typedef name for a struct, e.g. `kd_struct_Point`.
    pub fn c_name(&self, id: u32) -> String {
        format!("kd_struct_{}", self.defs[id as usize].name)
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// Structs in declaration (id) order, paired with their id.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &StructInfo)> {
        self.defs.iter().enumerate().map(|(i, s)| (i as u32, s))
    }

    // --- optionals (`?T`) --------------------------------------------------

    /// Intern an optional whose inner type is `inner`, returning the id used in
    /// `Type::Optional(id)`. Deduplicates structurally-equal optionals.
    pub fn intern_optional(&mut self, inner: Type) -> u32 {
        if let Some(i) = self.optional_inners.iter().position(|t| *t == inner) {
            return i as u32;
        }
        let id = self.optional_inners.len() as u32;
        self.optional_inners.push(inner);
        id
    }

    /// The inner type `T` of `?T` for `Type::Optional(id)`.
    pub fn optional_inner(&self, id: u32) -> Type {
        self.optional_inners[id as usize]
    }

    /// The C typedef name for `?T`, e.g. `kd_opt_int32_t` / `kd_opt_struct_Point`.
    pub fn optional_c_name(&self, id: u32) -> String {
        format!("kd_opt_{}", self.type_mangle(self.optional_inners[id as usize]))
    }

    /// A C-identifier-safe tag for a type, used to build composite type names.
    pub fn type_mangle(&self, t: Type) -> String {
        match t {
            Type::Struct(sid) => format!("struct_{}", self.defs[sid as usize].name),
            Type::Optional(oid) => format!("opt_{}", self.type_mangle(self.optional_inner(oid))),
            other => other.c_name().to_string(),
        }
    }

    /// All interned optionals, paired with their id, in interning order — the
    /// order the backend should emit their C typedefs.
    pub fn optionals(&self) -> impl Iterator<Item = (u32, Type)> + '_ {
        self.optional_inners
            .iter()
            .enumerate()
            .map(|(i, t)| (i as u32, *t))
    }
}
