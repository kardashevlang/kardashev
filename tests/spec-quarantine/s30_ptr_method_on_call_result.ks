// QUARANTINED (wave B, §30): the compiler contradicts SPEC §30.1/§30.2.
//
// SPEC §30.1: "Method calls likewise auto-deref a `*Struct` receiver" (general),
// and §30.2: "a receiver that is already a pointer is passed through" — so a
// method call whose receiver is a CALL RESULT of type `*Struct` should compile
// and mutate the pointee. sema ACCEPTS both programs below, but emit_c loses
// the receiver's struct identity and emits a call to `kd__add` (empty struct
// name) — the C compiler then fails:
//
//   error: implicit declaration of function 'kd__add'; did you mean 'kd_Acc_add'?
//   kd__add((*(kd_pick((&(kd_a))))), 9);
//
// (Note emit also auto-derefs the pointer and passes the struct BY VALUE, so
// even with the right name the mutation would be lost.) Receivers that are
// pointer-typed LOCALS or PARAMETERS work (pinned by
// tests/spec/s30_ptr_receivers/{ptr_local_method_passthrough,chain_via_ptr_locals_returning_self}.ks);
// only pointer-typed call-result receivers are broken.
//
// Expected per SPEC: prints 9 then 21 (and the chained variant prints 12).

const Acc = struct {
    total: i64,

    fn add(self: *Acc, v: i64) *Acc {
        self.total += v;
        return self;
    }
};

fn pick(p: *Acc) *Acc {
    return p;
}

pub fn main() void {
    var a: Acc = Acc{ .total = 0 };
    pick(&a).add(9);       // BROKEN: plain-fn call result as receiver
    print(a.total);        // expected 9
    a.add(5).add(7);       // BROKEN: method chain (a method-call result receiver)
    print(a.total);        // expected 21
}
