// bitwise.ks — bitwise & shift operators (v0.132).
//
//   a & b   a | b   a ^ b   a << n   a >> n   ~a
//
// Integer operators with C-like precedence: `| ^ &` sit below the comparisons,
// shifts sit just above `+ -`. Infix `&`/`|` are bitwise; prefix `&` is still
// address-of and `|x|` is still a capture. All fold in `const` expressions.

const FLAG_READ: i32 = 1 << 0;    // 1
const FLAG_WRITE: i32 = 1 << 1;   // 2
const FLAG_EXEC: i32 = 1 << 2;    // 4
const RW: i32 = FLAG_READ | FLAG_WRITE;   // 3, folded

fn has(flags: i32, flag: i32) bool {
    return (flags & flag) != 0;
}

// Population count (number of set bits) of a non-negative i32.
fn popcount(x: i32) i32 {
    var n: i32 = x;
    var count: i32 = 0;
    while (n != 0) : (n = n >> 1) {
        count += n & 1;
    }
    return count;
}

pub fn main() i32 {
    var perms: i32 = RW;
    perms = perms | FLAG_EXEC;     // now 7 (bitwise compound `|=` is later work)

    if (has(perms, FLAG_WRITE)) { print(1); } else { print(0); }   // 1
    if (has(perms, FLAG_EXEC))  { print(1); } else { print(0); }   // 1

    print(perms);                 // 7
    print(perms & FLAG_READ);     // 1
    print(perms ^ FLAG_WRITE);    // 5  (clears the write bit: 7 ^ 2)
    print(~0);                    // -1
    print(255 >> 4);              // 15
    print(popcount(255));         // 8
    print(popcount(1024));        // 1
    return 0;
}

test "bitwise" {
    expect((15 & 9) == 9);
    expect((1 << 10) == 1024);
    expect(popcount(7) == 3);
    expect(has(RW, FLAG_READ));
    expect(!has(FLAG_WRITE, FLAG_READ));
}
