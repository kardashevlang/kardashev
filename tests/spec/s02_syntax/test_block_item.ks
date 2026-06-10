//SPEC: §2 item := func | const_decl | test_block — test blocks parse beside functions; a program run executes only main
//OUT: 11
test "addition holds" {
    expect(2 + 2 == 4);
}

pub fn main() void {
    print(11);
}

test "ordering holds" {
    expect(1 < 2);
}
