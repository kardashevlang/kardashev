//SPEC: §27.3 the place is evaluated once, before the rhs — an rhs that mutates the index variable still writes the original element
//OUT: 60
//OUT: 20
//OUT: 1

// §27.3: the index hoist runs first, then `<place> = <place> <c-op> (<rhs>)`.
// `bump` flips `i` from 0 to 1 *while computing the rhs*; if the place were
// re-evaluated after the rhs, the write would land on a[1] instead of a[0].
fn bump(p: *usize) i64 {
    p.* = p.* + 1;
    return 50;
}

pub fn main() void {
    var a: [2]i64 = [2]i64{ 10, 20 };
    var i: usize = 0;
    a[i] += bump(&i);
    print(a[0]);   // 10 + 50 — the original element took the write
    print(a[1]);   // 20 — untouched
    print(i);      // 1 — the rhs side effect did happen
}
