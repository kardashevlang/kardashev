//SPEC: §41 x §20 a missing-file read yields the empty slice, classified into a union and switched on
//OUT: 0
//OUT: 1

const FileResult = union(enum) { content: []u8, missing: bool };

fn classify(data: []u8) FileResult {
    if (data.len == 0) { return FileResult{ .missing = true }; }
    return FileResult{ .content = data };
}

pub fn main() void {
    var a: Allocator = c_allocator();
    var data: []u8 = @readFile(a, "/tmp/kardc_spec_s92_missing.does_not_exist");
    print(data.len);   // 0 — nothing ever creates this path
    switch (classify(data)) {
        .content => |c| { print(c.len); },
        .missing => { print(1); },
    }
}
