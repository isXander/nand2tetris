/**
 * Test-script features, run against the Toggle chip. This file uses CRLF
 * line endings.
 */
LOAD Toggle.hdl,              // command names are case-insensitive
Output-File Script.out,
compare-to Script.cmp,
output-list time%S1.4.1 enable%B2.1.2 out%B2.1.2;   /* `enable` is truncated to the column width */

echo "toggling";
set enable 1,
repeat 3 {
    tick, tock, output;
}
clear-echo;

set enable 0, tick, tock, output!    // `!` pauses; the test runner resumes

breakpoint out 0,                     // fires when out becomes 0, below
while time <> %S"6" {
    tick, tock;
}
output;                               // the .cmp matches this row's time with `*`

output-list time%S1.4.1 out%D1.3.1;   // a new schema writes a new header
set enable 1, tick, output; tock, output;
