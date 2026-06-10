//SPEC: §13.2 an enum `switch` must cover every variant or carry an `else` arm
//ERR: E0210

const Color = enum { Red, Green, Blue };

pub fn main() void {
    var c: Color = .Red;
    switch (c) {           // .Blue is uncovered and there is no `else`
        .Red => { print(0); },
        .Green => { print(1); },
    }
}
