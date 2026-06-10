//SPEC: §1 `//` comments run to end of line and are ignored — even when they contain code text — and `//` inside a string literal is content, not a comment
//OUT: 21
//OUT: a//b
pub fn main() void {
    var total: i64 = 0; // total = total + 1000;
    // var total: i64 = 999;   <- commented-out redeclaration must not apply
    var i: i64 = 1; // operators in a comment: + - * / % << >> == != "quote
    while (i <= 6) { // loop bound is six
        total = total + i; // accumulate
        i = i + 1;
    }
    print(total); // 1+2+3+4+5+6 = 21; a broken comment would change this
    print("a//b"); // the // inside the string is string content
}
// a trailing comment may end the file without a newline