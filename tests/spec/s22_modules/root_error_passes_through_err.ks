//SPEC: §22.1 a ROOT-file parse error keeps its own code (E0200) — only IMPORTED-file errors wrap into E0294
//ERR: E0200

// The root both imports a (valid) module and contains its own syntax error
// (missing `;`): the diagnostic must surface as a plain E0200 against the
// root's source, not as a wrapped E0294.
@import("_basic_util.ks");

pub fn main() void {
    print(1)
}
