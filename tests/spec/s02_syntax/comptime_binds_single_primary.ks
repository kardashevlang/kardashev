//SPEC: §2 `comptime` binds a single primary — `comptime K + x` is (comptime K) + x; compound expressions need parens
//OUT: 42
//OUT: 5
const K: i64 = 12;

pub fn main() void {
    var x: i64 = 30;
    // If `comptime` captured the whole `K + x`, the runtime `x` would make
    // this E0130 (non-constant); binding only `K` it compiles and runs.
    var y: i64 = comptime K + x;
    print(y);
    print(comptime (2 + 3));
}
