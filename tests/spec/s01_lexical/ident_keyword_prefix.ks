//SPEC: §1 a spelling lexes as a keyword only on exact match — identifiers may begin with a keyword prefix
//OUT: 2310
//OUT: 24
pub fn main() void {
    var iffy: i64 = 2;        // prefix `if`
    var vary: i64 = 3;        // prefix `var`
    var breaker: i64 = 5;     // prefix `break`
    var testy: i64 = 7;       // prefix `test`
    var error_code: i64 = 11; // prefix `error`
    print(iffy * vary * breaker * testy * error_code);
    var constant: i64 = 4;    // prefix `const`
    var returned: i64 = 6;    // prefix `return`
    print(constant * returned);
}
