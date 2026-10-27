load DFF.hdl,
output-file PausesAndBreakpoints.out,
compare-to PausesAndBreakpoints.cmp,
output-list time%S1.4.1 in out;
breakpoint out 1,
set in 1, tick, tock, output!
set in 0, tick, tock, output;
