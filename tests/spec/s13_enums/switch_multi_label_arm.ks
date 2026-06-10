//SPEC: §13.1 one `switch` arm may carry several labels separated by `,` (any of them matches the arm)
//OUT: 2
//OUT: 0

const Day = enum { Mon, Tue, Wed, Thu, Fri, Sat, Sun };

fn is_weekend(d: Day) i64 {
    switch (d) {
        .Sat, .Sun => { return 1; },
        .Mon, .Tue, .Wed, .Thu, .Fri => { return 0; },
    }
}

pub fn main() void {
    // Both labels of the weekend arm match it...
    print(is_weekend(.Sat) + is_weekend(.Sun));
    // ...and all five labels of the weekday arm match the other.
    print(is_weekend(.Mon) + is_weekend(.Tue) + is_weekend(.Wed)
        + is_weekend(.Thu) + is_weekend(.Fri));
}
