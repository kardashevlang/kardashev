//SPEC: §35 `@panic(msg)` exits with code 101; output printed before the panic is flushed, statements after it never run
//EXIT: 101
//OUT: 11
//OUT: 22

pub fn main() void {
    print(11);
    print(22);
    @panic("boom");   // message goes to stderr; exit code is 101
    print(33);        // never reached — must NOT appear on stdout
}
