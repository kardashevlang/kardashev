//SPEC: §13.1 enums are first-class values: parameters, return types, and reassignable locals
//OUT: 12012

// A traffic-light state machine: `next` consumes and produces enum values.
// The 5-step trace encodes each visited state as a digit, so any wrong
// transition (or a `.V` literal resolving to the wrong variant in return
// position) changes the printed number.
const Light = enum { Red, Green, Yellow };

fn next(l: Light) Light {
    switch (l) {
        .Red => { return .Green; },
        .Green => { return .Yellow; },
        .Yellow => { return .Red; },
    }
}

fn code(l: Light) i64 {
    switch (l) {
        .Red => { return 0; },
        .Green => { return 1; },
        .Yellow => { return 2; },
    }
}

pub fn main() void {
    var l: Light = .Red;
    var trace: i64 = 0;
    var i: i64 = 0;
    while (i < 5) : (i = i + 1) {
        l = next(l);
        trace = trace * 10 + code(l);
    }
    print(trace);   // Green,Yellow,Red,Green,Yellow -> 1,2,0,1,2
}
