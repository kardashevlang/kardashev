//SPEC: §15.2 `base[lo..hi]` requires an array or slice base — slicing a scalar is E0232
//ERR: E0232

pub fn main() void {
    var x: i64 = 5;
    var s: []i64 = x[0..1]; // an i64 is neither an array nor a slice
    print(s.len);
}
