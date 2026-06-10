//SPEC: §32.1 builtins are not constant expressions — a top-level `const` cannot fold `@sizeOf` / `@typeName`
//ERR: E0130
const S = @sizeOf(i64);
const N = @typeName(i64);

pub fn main() void {
    print(1);
}
