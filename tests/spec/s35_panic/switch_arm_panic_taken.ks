//SPEC: §35.2 an arm whose body is `@panic` diverges when TAKEN: exit 101, with all pre-arm output intact
//EXIT: 101
//OUT: 10

fn classify(n: i64) i64 {
    switch (n % 2) {
        0 => { return 10; },
        1 => { return 20; },
        // C remainder: a negative n gives -1, landing here.
        else => { @panic("negative remainder"); },
    }
}

pub fn main() void {
    print(classify(4));       // 10
    print(classify(0 - 3));   // -3 % 2 == -1 -> panic arm
}
