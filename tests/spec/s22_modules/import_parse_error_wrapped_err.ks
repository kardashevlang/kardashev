//SPEC: §22.1 a PARSE error inside an imported file is bundled into one E0294 naming that file
//ERR: E0294

@import("_broken_parse.ks");

pub fn main() void {
    print(1);
}
