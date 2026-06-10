//SPEC: §23.1+§22.1 byte equality is the std library fn `str_eq` (strings have no `==` operator — see s23 quarantine)
//OUT: 1
//OUT: 0
//OUT: 1

@import("std");

pub fn main() void {
    if (str_eq("kardashev", "kardashev")) { print(1); } else { print(0); }
    if (str_eq("kardashev", "kardacheff")) { print(1); } else { print(0); }
    if (str_eq("", "")) { print(1); } else { print(0); }
}
