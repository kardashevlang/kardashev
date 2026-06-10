//SPEC: §28.2 `& | ^ << ~` operate at u8/u16/u32 width — results stored into the operand type
//OUT: 136
//OUT: 238
//OUT: 102
//OUT: 85
//OUT: 144
//OUT: 170
//OUT: 43775
//OUT: 43605
//OUT: 15728880
//OUT: 4043305215
//OUT: 4027576335

pub fn main() void {
    // u8: 170 = 0b10101010, 204 = 0b11001100
    var a8: u8 = 170;
    var b8: u8 = 204;
    var r1: u8 = a8 & b8;
    var r2: u8 = a8 | b8;
    var r3: u8 = a8 ^ b8;
    print(r1);  // 136
    print(r2);  // 238
    print(r3);  // 102
    var r4: u8 = ~a8;       // complement at u8 width: 85
    print(r4);
    var c8: u8 = 200;
    var r5: u8 = c8 << 1;   // 400 mod 2^8 = 144 — the result is a u8
    print(r5);

    // u16: 43690 = 0xAAAA, masked against the low byte
    var a16: u16 = 43690;
    var b16: u16 = 255;
    var s1: u16 = a16 & b16;
    var s2: u16 = a16 | b16;
    var s3: u16 = a16 ^ b16;
    print(s1);  // 170
    print(s2);  // 43775
    print(s3);  // 43605

    // u32: 0xF0F0F0F0 vs 0x00FF00FF
    var a32: u32 = 4042322160;
    var b32: u32 = 16711935;
    var t1: u32 = a32 & b32;
    var t2: u32 = a32 | b32;
    var t3: u32 = a32 ^ b32;
    print(t1);  // 15728880
    print(t2);  // 4043305215
    print(t3);  // 4027576335
}
