// labeled_loops.ks — labeled break / continue (v0.147).
//
//   outer: while (…) {
//       while (…) {
//           break :outer;      // leave BOTH loops at once
//           continue :outer;   // jump to the next OUTER iteration
//       }
//   }
//
// A `name:` before a `while`/`for` labels the loop; `break`/`continue` may then
// target it. Unlabeled `break`/`continue` still mean the innermost loop.

// Is `n` (>= 2) prime? Trial-divide; a labeled break stops the search early.
fn is_prime(n: i32) bool {
    var d: i32 = 2;
    var prime: bool = true;
    search: while (d < n) : (d += 1) {
        if (n % d == 0) {
            prime = false;
            break :search;
        }
    }
    return prime;
}

// Count grid cells (i, j) with i,j in 0..size and j < i (strictly lower
// triangle), using `continue :rows` to skip the rest of a row.
fn lower_triangle(size: i32) i32 {
    var count: i32 = 0;
    var i: i32 = 0;
    rows: while (i < size) : (i += 1) {
        var j: i32 = 0;
        while (j < size) : (j += 1) {
            if (j >= i) {
                continue :rows;
            }
            count += 1;
        }
    }
    return count;
}

pub fn main() i32 {
    if (is_prime(13)) { print(1); } else { print(0); }   // 1
    if (is_prime(15)) { print(1); } else { print(0); }   // 0
    if (is_prime(2))  { print(1); } else { print(0); }   // 1
    print(lower_triangle(5));    // 0+1+2+3+4 = 10
    print(lower_triangle(4));    // 0+1+2+3   = 6
    return 0;
}

test "labeled loops" {
    expect(is_prime(7));
    expect(!is_prime(9));
    expect(is_prime(97));
    expect(lower_triangle(6) == 15);
    expect(lower_triangle(1) == 0);
}
