load And.hdl,
output-file Variables.out,
output-list a b out time;
set c 1,
set out 1,
set a 2,
set a -2,
set time 1,
breakpoint time 3,
breakpoint out %S"1",
while time < %S"5" { tick, tock; }
while out = %S"1" { eval; }
