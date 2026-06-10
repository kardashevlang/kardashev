//SPEC: §13.2 a duplicated integer `switch` label is rejected
//ERR: E0211

pub fn main() void {
    var n: i64 = 3;
    switch (n) {
        1 => { print(1); },
        2 => { print(2); },
        1 => { print(3); },     // duplicate label
        else => {},
    }
}
