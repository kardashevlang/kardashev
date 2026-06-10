//SPEC: §13.2 the duplicate-variant rule is per enum — two enums may declare the same variant name, context disambiguates `.V`
//OUT: 11
//OUT: 32

// Both enums declare `Ok`; each `.Ok` literal resolves against the enum the
// parameter expects, and each `switch` accepts only its own scrutinee's
// variants. If the namespaces collided, this would not even compile.
const Parse = enum { Ok, Fail };
const Net = enum { Ok, Timeout, Refused };

fn pscore(p: Parse) i64 {
    switch (p) {
        .Ok => { return 1; },
        .Fail => { return 2; },
    }
}

fn nscore(n: Net) i64 {
    switch (n) {
        .Ok => { return 10; },
        .Timeout => { return 20; },
        .Refused => { return 30; },
    }
}

pub fn main() void {
    print(pscore(.Ok) + nscore(.Ok));          // 1 + 10
    print(pscore(.Fail) + nscore(.Refused));   // 2 + 30
}
