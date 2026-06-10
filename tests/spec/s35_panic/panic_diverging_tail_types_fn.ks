//SPEC: §35.2 a value-returning function may END in `@panic` — the divergence suppresses the fall-through, so no return is needed after it
//OUT: 8

fn f(n: i64) i64 {
    if (n > 0) {
        return n * 2;
    }
    @panic("non-positive");   // the only way past the if; no `return` after
}

pub fn main() void {
    print(f(4));   // good path: the panic tail is never reached
}
