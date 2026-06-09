// for_loops.ks — `for` loops over arrays & slices (v0.133).
//
//   for (xs) |x| { … }            iterate elements (x is a by-value copy)
//   for (xs, 0..) |x, i| { … }    also bind a 0-based usize index
//
// Works for both `[N]T` arrays and `[]T` slices; lowers to an indexed `while`,
// so `break` / `continue` behave (and `continue` still advances the index).

fn sum(xs: []i32) i32 {
    var total: i32 = 0;
    for (xs) |v| {
        total += v;
    }
    return total;
}

fn max(xs: []i32) i32 {
    var best: i32 = xs[0];
    for (xs) |v| {
        if (v > best) {
            best = v;
        }
    }
    return best;
}

// First index whose element equals `target`, or len if absent.
fn index_of(xs: []i32, target: i32) usize {
    for (xs, 0..) |v, i| {
        if (v == target) {
            return i;
        }
    }
    return xs.len;
}

pub fn main() i32 {
    var data: [6]i32 = [6]i32{ 4, 8, 15, 16, 23, 42 };
    var all: []i32 = data[0..6];

    print(sum(all));              // 108
    print(max(all));              // 42
    print(index_of(all, 16));     // 3
    print(index_of(all, 99));     // 6 (not found → len)

    // `continue` skips odds; the index still advances each iteration.
    var odd_count: i32 = 0;
    for (all) |v| {
        if (v % 2 == 0) {
            continue;
        }
        odd_count += 1;           // 15, 23
    }
    print(odd_count);             // 2

    // Iterate an array directly (no slicing).
    var letters: [3]i32 = [3]i32{ 1, 2, 3 };
    var acc: i32 = 0;
    for (letters, 0..) |v, i| {
        acc += v * v;             // 1 + 4 + 9
        if (i == 2) {
            acc += 100;           // index-aware
        }
    }
    print(acc);                   // 114
    return 0;
}

test "for loops" {
    var xs: [4]i32 = [4]i32{ 5, 10, 15, 20 };
    var s: []i32 = xs[0..4];
    expect(sum(s) == 50);
    expect(max(s) == 20);
    expect(index_of(s, 15) == 2);
    var last: usize = 0;
    for (s, 0..) |v, i| {
        last = i;
    }
    expect(last == 3);            // continue/advance reached the final index
}
