//SPEC: §2 test_block := "test" STRING block — a test block cannot be marked `pub` (E0201)
//ERR: E0201
// The parser reports E0201 here, but modules::resolve wraps every root-file
// (see tests/spec-quarantine for the exact-code pin of this discrepancy).
pub test "not allowed" {
    expect(true);
}

pub fn main() void {
    print(1);
}
