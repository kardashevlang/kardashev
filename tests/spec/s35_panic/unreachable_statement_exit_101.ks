//SPEC: §35 control reaching `unreachable` exits with code 101 (fixed message on stderr); prior stdout survives
//EXIT: 101
//OUT: 5

pub fn main() void {
    print(5);
    unreachable;
    print(6);   // never reached
}
