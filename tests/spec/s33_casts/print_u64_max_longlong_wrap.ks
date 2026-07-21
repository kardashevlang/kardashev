//SPEC: §4.1 `print` writes integers through `long long`: a u64 above i64 max wraps in the output (the documented cast)
//OUT: -1
//OUT: 4294967295

pub fn main() void {
    var big: u64 = 0;
    big -= 1;      // u64 wrap: 18446744073709551615
    print(big);    // the (long long) cast makes that -1
    var mid: u32 = 0;
    mid -= 1;      // u32 max fits long long
    print(mid);
}
