//SPEC: §12 x §20 `!Union`: the success path carries a union payload; the error path constructs a fallback union via the eager catch default
//OUT: 30
//OUT: 7

const R = union(enum) { val: i64, none: bool };

fn fetch(ok: bool) !R {
    if (ok) { return R{ .val = 30 }; }
    return error.Nope;
}

fn show(r: R) void {
    switch (r) {
        .val => |v| { print(v); },
        .none => { print(7); },   // an arm may ignore its payload
    }
}

pub fn main() void {
    var a: R = fetch(true) catch R{ .none = true };
    show(a);
    var b: R = fetch(false) catch R{ .none = true };
    show(b);
}
