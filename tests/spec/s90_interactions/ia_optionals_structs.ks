//SPEC: §11 x §9 — a ?Struct-typed FIELD: struct payload widens into it, captures destructure it, null resets it
//OUT: 44
//OUT: 2
//OUT: 56
//OUT: -1

// Wave A pinned ?i64 fields and ?Struct returns; this is the composition: a
// struct whose FIELD is an optional of ANOTHER struct.
const Inner = struct {
    x: i64,
    y: i64,
};

const Outer = struct {
    tag: i64,
    inner: ?Inner,
};

fn make(n: i64) Outer {
    if (n % 2 == 0) {
        return Outer{ .tag = n, .inner = null };
    }
    // The Inner literal widens to the ?Inner field at the init site.
    return Outer{ .tag = n, .inner = Inner{ .x = n, .y = n * n } };
}

pub fn main() void {
    var total: i64 = 0;
    var nulls: i64 = 0;
    var i: i64 = 1;
    while (i <= 5) : (i += 1) {
        var o: Outer = make(i);
        if (o.inner) |inn| {
            total += inn.x + inn.y;   // captured struct payload's fields
        } else {
            nulls += 1;
        }
    }
    print(total);                     // (1+1) + (3+9) + (5+25) = 44
    print(nulls);                     // i = 2, 4

    var o2: Outer = make(2);          // starts null
    o2.inner = Inner{ .x = 7, .y = 8 };  // widening write to the field place
    if (o2.inner) |inn| {
        print(inn.x * inn.y);         // 56
    }
    o2.inner = null;                  // reset
    if (o2.inner) |inn| {
        print(inn.x);
    } else {
        print(0 - 1);
    }
}
