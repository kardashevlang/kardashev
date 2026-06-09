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
    /// `!T` — an error union (implicit global error set, v0.115). The `u32`
    /// indexes the error-union-payload table (`error_union_payload`).
    ErrorUnion(u32),
    /// A plain (C-like) enum, v0.116. The `u32` indexes the enum table
    /// (`enum_info`).
    Enum(u32),
    /// `[N]T` — a fixed-size array (v0.117). The `u32` indexes the array table
    /// (`array_info`: element type + length).
    Array(u32),
    /// `*T` — a single pointer (v0.118). The `u32` indexes the pointee table.
    Ptr(u32),
    /// `[]T` — a slice (`{ptr,len}` view, v0.118). The `u32` indexes the slice
    /// element table.
    Slice(u32),
    /// The `Allocator` interface value (v0.119). A first-class, explicitly
    /// passed allocator; `c_allocator()` constructs one backed by malloc/free.
    Allocator,
    /// A tagged union `union(enum) { … }` (v0.124). The `u32` indexes the union
    /// table (`union_info`: variant names + payload types).
    Union(u32),
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
            "Allocator" => Type::Allocator,
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
            Type::ErrorUnion(_) => "error union",
            Type::Enum(_) => "enum",
            Type::Array(_) => "array",
            Type::Ptr(_) => "pointer",
            Type::Slice(_) => "slice",
            Type::Allocator => "Allocator",
            Type::Union(_) => "union",
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
            Type::ErrorUnion(_) => {
                unreachable!("c_name on an error-union type; use StructTable::error_union_c_name")
            }
            Type::Enum(_) => unreachable!("c_name on an enum type; use StructTable::enum_c_name"),
            Type::Array(_) => {
                unreachable!("c_name on an array type; use StructTable::array_c_name")
            }
            Type::Ptr(_) => unreachable!("c_name on a pointer type; use StructTable::ptr_c_name"),
            Type::Slice(_) => {
                unreachable!("c_name on a slice type; use StructTable::slice_c_name")
            }
            Type::Allocator => "kd_allocator",
            Type::Union(_) => unreachable!("c_name on a union type; use StructTable::union_c_name"),
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

/// A resolved plain-enum definition: its name and ordered variant names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumInfo {
    pub name: String,
    pub variants: Vec<String>,
}

impl EnumInfo {
    /// The 0-based index (and C value) of `variant`, if present.
    pub fn variant_index(&self, variant: &str) -> Option<usize> {
        self.variants.iter().position(|v| v == variant)
    }
}

/// A resolved tagged-union definition: name + ordered `(variant, payload type)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnionInfo {
    pub name: String,
    pub variants: Vec<(String, Type)>,
}

impl UnionInfo {
    /// The 0-based tag index of `variant`, if present.
    pub fn variant_index(&self, variant: &str) -> Option<usize> {
        self.variants.iter().position(|(n, _)| n == variant)
    }

    /// The payload type of `variant`, if present.
    pub fn payload_type(&self, variant: &str) -> Option<Type> {
        self.variants.iter().find(|(n, _)| n == variant).map(|(_, t)| *t)
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
    /// Payload types of `!T` error unions, indexed by `Type::ErrorUnion(id)`.
    error_union_payloads: Vec<Type>,
    /// The (implicit global) error set: declared error names, 1-based codes
    /// (`error_names[0]` has code 1; code 0 means "no error").
    error_names: Vec<String>,
    /// Plain enum definitions, indexed by the id in `Type::Enum(id)`.
    enum_defs: Vec<EnumInfo>,
    enum_by_name: HashMap<String, u32>,
    /// Tagged-union definitions, indexed by the id in `Type::Union(id)` (v0.124).
    union_defs: Vec<UnionInfo>,
    union_by_name: HashMap<String, u32>,
    /// Array types `(element, length)`, indexed by the id in `Type::Array(id)`.
    array_info: Vec<(Type, usize)>,
    /// Pointee types, indexed by the id in `Type::Ptr(id)` (v0.118).
    ptr_pointees: Vec<Type>,
    /// Slice element types, indexed by the id in `Type::Slice(id)` (v0.118).
    slice_elems: Vec<Type>,
    /// Monomorphisation instantiations of generic functions (v0.120): each is a
    /// `(generic fn name, concrete type arguments)` pair the backend must emit.
    instantiations: Vec<Instantiation>,
    /// Type aliases `const Alias = Name(C);` → the aliased type (v0.129). Shared
    /// from sema to the backend so an alias name resolves in both.
    type_aliases: HashMap<String, Type>,
}

/// One comptime argument to a generic function: a type (`comptime T: type`,
/// v0.120) or a value (`comptime n: usize`, v0.128).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComptimeArg {
    Type(Type),
    Value(i64),
}

/// One monomorphised instantiation of a generic function (v0.120 / v0.128).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instantiation {
    pub fn_name: String,
    pub args: Vec<ComptimeArg>,
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
            Type::ErrorUnion(eid) => {
                format!("err_{}", self.type_mangle(self.error_union_payload(eid)))
            }
            Type::Enum(eid) => format!("enum_{}", self.enum_defs[eid as usize].name),
            Type::Union(uid) => format!("union_{}", self.union_defs[uid as usize].name),
            Type::Array(aid) => {
                let (elem, len) = self.array_info[aid as usize];
                format!("arr_{}_{}", self.type_mangle(elem), len)
            }
            Type::Ptr(pid) => format!("ptr_{}", self.type_mangle(self.ptr_pointees[pid as usize])),
            Type::Slice(sid) => {
                format!("slice_{}", self.type_mangle(self.slice_elems[sid as usize]))
            }
            other => other.c_name().to_string(),
        }
    }

    // --- enums (v0.116) ----------------------------------------------------

    /// Register enum `name`, returning its id (existing id if already interned;
    /// variants are filled later via `set_enum_variants`).
    pub fn intern_enum(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.enum_by_name.get(name) {
            return id;
        }
        let id = self.enum_defs.len() as u32;
        self.enum_defs.push(EnumInfo {
            name: name.to_string(),
            variants: Vec::new(),
        });
        self.enum_by_name.insert(name.to_string(), id);
        id
    }

    pub fn enum_id_of(&self, name: &str) -> Option<u32> {
        self.enum_by_name.get(name).copied()
    }

    pub fn enum_get(&self, id: u32) -> &EnumInfo {
        &self.enum_defs[id as usize]
    }

    pub fn set_enum_variants(&mut self, id: u32, variants: Vec<String>) {
        self.enum_defs[id as usize].variants = variants;
    }

    /// The C typedef name for an enum, e.g. `kd_enum_Color`.
    pub fn enum_c_name(&self, id: u32) -> String {
        format!("kd_enum_{}", self.enum_defs[id as usize].name)
    }

    /// The C enumerator name for a variant, e.g. `kd_enum_Color_Red`.
    pub fn enum_variant_c_name(&self, id: u32, variant: &str) -> String {
        format!("kd_enum_{}_{}", self.enum_defs[id as usize].name, variant)
    }

    /// Enums in declaration (id) order, paired with their id.
    pub fn enums(&self) -> impl Iterator<Item = (u32, &EnumInfo)> {
        self.enum_defs.iter().enumerate().map(|(i, e)| (i as u32, e))
    }

    // --- tagged unions (v0.124) -------------------------------------------

    pub fn intern_union(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.union_by_name.get(name) {
            return id;
        }
        let id = self.union_defs.len() as u32;
        self.union_defs.push(UnionInfo {
            name: name.to_string(),
            variants: Vec::new(),
        });
        self.union_by_name.insert(name.to_string(), id);
        id
    }

    pub fn union_id_of(&self, name: &str) -> Option<u32> {
        self.union_by_name.get(name).copied()
    }

    pub fn union_get(&self, id: u32) -> &UnionInfo {
        &self.union_defs[id as usize]
    }

    pub fn set_union_variants(&mut self, id: u32, variants: Vec<(String, Type)>) {
        self.union_defs[id as usize].variants = variants;
    }

    /// The C typedef name for a union, e.g. `kd_union_Shape`.
    pub fn union_c_name(&self, id: u32) -> String {
        format!("kd_union_{}", self.union_defs[id as usize].name)
    }

    /// Unions in declaration (id) order, paired with their id.
    pub fn unions(&self) -> impl Iterator<Item = (u32, &UnionInfo)> {
        self.union_defs.iter().enumerate().map(|(i, u)| (i as u32, u))
    }

    // --- arrays `[N]T` (v0.117) -------------------------------------------

    /// Intern array type `[len]elem`, returning its id (deduplicated).
    pub fn intern_array(&mut self, elem: Type, len: usize) -> u32 {
        if let Some(i) = self.array_info.iter().position(|&(e, l)| e == elem && l == len) {
            return i as u32;
        }
        let id = self.array_info.len() as u32;
        self.array_info.push((elem, len));
        id
    }

    /// The element type of `Type::Array(id)`.
    pub fn array_elem(&self, id: u32) -> Type {
        self.array_info[id as usize].0
    }

    /// The length of `Type::Array(id)`.
    pub fn array_len(&self, id: u32) -> usize {
        self.array_info[id as usize].1
    }

    /// The C typedef name for `[N]T`, e.g. `kd_arr_int32_t_3`.
    pub fn array_c_name(&self, id: u32) -> String {
        let (elem, len) = self.array_info[id as usize];
        format!("kd_arr_{}_{}", self.type_mangle(elem), len)
    }

    /// All interned arrays, paired with id, in interning order.
    pub fn arrays(&self) -> impl Iterator<Item = (u32, Type, usize)> + '_ {
        self.array_info
            .iter()
            .enumerate()
            .map(|(i, &(e, l))| (i as u32, e, l))
    }

    // --- pointers `*T` & slices `[]T` (v0.118) ----------------------------

    pub fn intern_ptr(&mut self, pointee: Type) -> u32 {
        if let Some(i) = self.ptr_pointees.iter().position(|t| *t == pointee) {
            return i as u32;
        }
        let id = self.ptr_pointees.len() as u32;
        self.ptr_pointees.push(pointee);
        id
    }

    pub fn ptr_pointee(&self, id: u32) -> Type {
        self.ptr_pointees[id as usize]
    }

    /// The C name for `*T`, e.g. `int32_t*`. (Pointers need no typedef.)
    pub fn ptr_c_name(&self, id: u32, base_cty: &str) -> String {
        let _ = id;
        format!("{}*", base_cty)
    }

    pub fn intern_slice(&mut self, elem: Type) -> u32 {
        if let Some(i) = self.slice_elems.iter().position(|t| *t == elem) {
            return i as u32;
        }
        let id = self.slice_elems.len() as u32;
        self.slice_elems.push(elem);
        id
    }

    pub fn slice_elem(&self, id: u32) -> Type {
        self.slice_elems[id as usize]
    }

    /// The C typedef name for `[]T`, e.g. `kd_slice_int32_t`.
    pub fn slice_c_name(&self, id: u32) -> String {
        format!("kd_slice_{}", self.type_mangle(self.slice_elems[id as usize]))
    }

    /// All interned slices, paired with id + element, in interning order.
    pub fn slices(&self) -> impl Iterator<Item = (u32, Type)> + '_ {
        self.slice_elems
            .iter()
            .enumerate()
            .map(|(i, t)| (i as u32, *t))
    }

    // --- generic-function instantiations (v0.120) -------------------------

    /// Record a monomorphisation instantiation; returns true if newly added.
    pub fn intern_instantiation(&mut self, fn_name: &str, args: Vec<ComptimeArg>) -> bool {
        if self
            .instantiations
            .iter()
            .any(|i| i.fn_name == fn_name && i.args == args)
        {
            return false;
        }
        self.instantiations.push(Instantiation {
            fn_name: fn_name.to_string(),
            args,
        });
        true
    }

    /// All recorded instantiations, in discovery order.
    pub fn instantiations(&self) -> &[Instantiation] {
        &self.instantiations
    }

    /// Record a type alias `Alias` → `ty` (v0.129).
    pub fn add_alias(&mut self, name: &str, ty: Type) {
        self.type_aliases.insert(name.to_string(), ty);
    }

    /// The type a type-alias name refers to, if any (v0.129).
    pub fn alias_of(&self, name: &str) -> Option<Type> {
        self.type_aliases.get(name).copied()
    }

    /// The C name for an instantiation, e.g. `kd_max__int32_t` or
    /// `kd_zeros__5` (a comptime value arg mangles to its decimal digits).
    pub fn instantiation_c_name(&self, inst: &Instantiation) -> String {
        let mut s = format!("kd_{}__", inst.fn_name);
        for (i, a) in inst.args.iter().enumerate() {
            if i > 0 {
                s.push('_');
            }
            match a {
                ComptimeArg::Type(t) => s.push_str(&self.type_mangle(*t)),
                ComptimeArg::Value(v) => s.push_str(&v.to_string()),
            }
        }
        s
    }

    // --- error unions (`!T`) + the implicit global error set --------------

    /// Intern error name `name`, returning its 1-based code (0 = "no error").
    pub fn intern_error(&mut self, name: &str) -> u32 {
        if let Some(i) = self.error_names.iter().position(|n| n == name) {
            return i as u32 + 1;
        }
        self.error_names.push(name.to_string());
        self.error_names.len() as u32
    }

    /// The 1-based code of error `name`, if declared.
    pub fn error_code(&self, name: &str) -> Option<u32> {
        self.error_names.iter().position(|n| n == name).map(|i| i as u32 + 1)
    }

    /// All declared error names paired with their 1-based code.
    pub fn errors(&self) -> impl Iterator<Item = (u32, &str)> + '_ {
        self.error_names
            .iter()
            .enumerate()
            .map(|(i, n)| (i as u32 + 1, n.as_str()))
    }

    /// Intern an `!T` error union with payload `payload`, returning its id.
    pub fn intern_error_union(&mut self, payload: Type) -> u32 {
        if let Some(i) = self.error_union_payloads.iter().position(|t| *t == payload) {
            return i as u32;
        }
        let id = self.error_union_payloads.len() as u32;
        self.error_union_payloads.push(payload);
        id
    }

    /// The payload type `T` of `!T` for `Type::ErrorUnion(id)`.
    pub fn error_union_payload(&self, id: u32) -> Type {
        self.error_union_payloads[id as usize]
    }

    /// The C typedef name for `!T`, e.g. `kd_err_int32_t`.
    pub fn error_union_c_name(&self, id: u32) -> String {
        format!(
            "kd_err_{}",
            self.type_mangle(self.error_union_payloads[id as usize])
        )
    }

    /// All interned error unions, paired with id, in interning order.
    pub fn error_unions(&self) -> impl Iterator<Item = (u32, Type)> + '_ {
        self.error_union_payloads
            .iter()
            .enumerate()
            .map(|(i, t)| (i as u32, *t))
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
