//SPEC: §30.2 a pointer-receiver call's receiver may be a CALL RESULT of type `*Struct` (passed through, mutation real) — and chains through a method returning `*Self`
//OUT: 9
//OUT: 21
//OUT: 30

// Was quarantined (wave B, v0.156): emit_c's receiver-struct resolution
// (`struct_of_expr`) only unwrapped `Type::Struct` returns, so a `Call` /
// `MethodCall` receiver returning `*Acc` / `*Self` resolved to an empty
// struct name (`kd__add`, implicit-declaration cc failure) — and, with the
// method params unresolved, the pointer was auto-deref'd and passed BY VALUE.
// Fixed: a `*Struct` return resolves to its pointee (SPEC §30.1), the pointer
// passes through unchanged, and the mutation lands in the caller's struct.

const Acc = struct {
    total: i64,

    fn add(self: *Acc, v: i64) *Acc {
        self.total += v;
        return self;
    }

    fn get(self: Acc) i64 {
        return self.total;
    }
};

fn pick(p: *Acc) *Acc {
    return p;
}

pub fn main() void {
    var a: Acc = Acc{ .total = 0 };
    pick(&a).add(9);       // plain-fn call result as receiver
    print(a.total);        // 9
    a.add(5).add(7);       // method chain (a method-call result receiver)
    print(a.total);        // 21
    print(pick(&a).add(9).get()); // chain off a call result + value-receiver tail: 30
}
