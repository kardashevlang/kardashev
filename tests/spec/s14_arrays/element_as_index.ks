//SPEC: §14.2 an index may be any integer expression — including another array's element
//OUT: 30104020

// Apply the permutation {2,0,3,1} to {10,20,30,40} by using one array's
// elements to index the other, packing the visits into one number:
// vals[2]=30, vals[0]=10, vals[3]=40, vals[1]=20 -> 30 10 40 20.
pub fn main() void {
    var perm: [4]i64 = [4]i64{ 2, 0, 3, 1 };
    var vals: [4]i64 = [4]i64{ 10, 20, 30, 40 };
    var out: i64 = 0;
    var i: usize = 0;
    while (i < 4) : (i = i + 1) {
        out = out * 100 + vals[perm[i]];
    }
    print(out);
}
