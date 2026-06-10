//SPEC: §34.1 a named error set combines with a `void` payload (`E!void`) — same payload-less runtime shape as `!void`
//OUT: -4
//OUT: 1

const E = error{Boom};

fn f(n: i64) E!void {
    if (n > 0) {
        return error.Boom;
    }
    print(n);
}

pub fn main() void {
    f(0 - 4) catch print(0 - 1); // success: prints -4, handler not run
    f(2) catch |e| print(e);     // error path: Boom is code 1
}
