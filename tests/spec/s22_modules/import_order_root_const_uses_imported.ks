//SPEC: §22.1 imported items PRECEDE the importer's own — a root const initializer folds over an imported const
//OUT: 42

// §3 const-eval only sees consts defined EARLIER in the flat order; if the
// importer's items came first, ORD_BASE would be a forward reference (E0131).
@import("_ord_def.ks");

const ORD_DERIVED = ORD_BASE * 6;

pub fn main() void {
    print(ORD_DERIVED);
}
