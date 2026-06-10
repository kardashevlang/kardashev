//SPEC: §32.1 an unknown `@name(…)` in expression position is an error
//ERR: E0320
pub fn main() void {
    var x: i64 = @bogus(1);
    print(x);
}
