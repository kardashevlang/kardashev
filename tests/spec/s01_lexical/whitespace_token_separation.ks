//SPEC: §1 whitespace (space, tab, CR, LF) separates tokens and is otherwise ignored — dense and spread-out spellings of one expression agree
//OUT: 5
//OUT: 38
pub fn main() void {
    // No spaces at all between tokens.
    print(1+2*3-4/2);
    // The same kind of expression spread over lines with tabs.
    var n: i64 =
	(	7
		+
	12	)
	*	2	;
    print(n);
}
