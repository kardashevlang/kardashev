//SPEC: §39.1 ranges do not establish exhaustiveness — an integer `switch` still requires `else`
//ERR: E0214

pub fn main() void {
    var n: i64 = 1;
    switch (n) {
        0..9 => { print(1); },   // covers the inputs we'd use — irrelevant: no else
    }
}
