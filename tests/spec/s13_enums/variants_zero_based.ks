//SPEC: §13.1 variants without explicit values are numbered 0,1,2,… in declaration order
//OUT: 0
//OUT: 3
//OUT: 12

const Suit = enum { Clubs, Diamonds, Hearts, Spades };

pub fn main() void {
    print(@intFromEnum(Suit.Clubs));    // first variant is 0
    print(@intFromEnum(Suit.Spades));   // fourth is 3
    // Diamonds = 1, Hearts = 2: positional, not alphabetical or hashed.
    var mixed: i64 = @intFromEnum(Suit.Diamonds) * 10 + @intFromEnum(Suit.Hearts);
    print(mixed);
}
