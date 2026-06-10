//SPEC: §28.2 `~x` and `x << n` yield the OPERAND's type: narrow (8/16-bit) results truncate back to the operand width (two's-complement, like @as §33) instead of leaking C's int promotion
//OUT: 85
//OUT: 144

pub fn main() void {
    var x: u8 = 170;
    print(~x);
    var z: u8 = 200;
    print(z << 1);
}
