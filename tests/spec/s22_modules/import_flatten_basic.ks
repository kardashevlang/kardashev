//SPEC: §22.1 `@import("p.ks")` flattens the file's items into one module — the importer uses them by bare name
//OUT: 42
//OUT: 10

@import("_basic_util.ks");

pub fn main() void {
    print(util_add(20, 22));   // imported fn
    print(UTIL_K * 2);         // imported const: 5 * 2
}
