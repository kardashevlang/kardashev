//SPEC: §2/§3 a block statement is a scope — sibling blocks may each declare a local of the same name
//OUT: 13
//
// QUARANTINED (v0.155 corpus, s02 author): sema accepts this program (each
// block is its own scope on the §3 scope stack), but emit_c's Stmt::Block
// lowering (emit_block, called via `Stmt::Block(b) => self.emit_block(b,
// Scope::plain())`) emits NO C braces around the block's statements, so both
// `kd_inner` definitions land in the same C scope and cc fails with
// "redefinition of 'kd_inner'". A sema-valid program that cannot be built
// violates the observable contract; fix = wrap Stmt::Block bodies in `{ }`.
pub fn main() void {
    var total: i64 = 0;
    {
        var inner: i64 = 6;
        total = total + inner;
    }
    {
        var inner: i64 = 7;
        total = total + inner;
    }
    print(total);
}
