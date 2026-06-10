//SPEC: §17 x §15 — one generic in-place sort serves []i64 and []u8 (string bytes); writes land in the backing stores
//OUT: 2
//OUT: 4
//OUT: 9
//OUT: 15
//OUT: 31
//OUT: aadehkrsv

// Insertion sort over []T: comparisons, swaps and indexing all run on the
// substituted element type. The u8 instance sorts a heap copy of a string's
// bytes and prints it back as a string.
fn isort(comptime T: type, xs: []T) void {
    var i: usize = 1;
    while (i < xs.len) : (i += 1) {
        var j: usize = i;
        while (j > 0) {
            if (xs[j - 1] <= xs[j]) {
                break;
            }
            var t: T = xs[j - 1];
            xs[j - 1] = xs[j];
            xs[j] = t;
            j -= 1;
        }
    }
}

pub fn main() void {
    var nums: [5]i64 = [5]i64{ 31, 4, 15, 9, 2 };
    isort(i64, nums[0..5]);          // the slice aliases the array
    var k: usize = 0;
    while (k < 5) : (k += 1) {
        print(nums[k]);              // sorted IN the array: 2 4 9 15 31
    }

    var a: Allocator = c_allocator();
    var src: []u8 = "kardashev";
    var buf: []u8 = alloc(a, u8, src.len);
    var m: usize = 0;
    while (m < src.len) : (m += 1) {
        buf[m] = src[m];
    }
    isort(u8, buf);
    print(buf);                      // bytes of "kardashev" sorted: aadehkrsv
    free(a, buf);
}
