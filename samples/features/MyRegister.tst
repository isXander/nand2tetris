load MyRegister.hdl,
output-file MyRegister.out,
compare-to MyRegister.cmp,
output-list time%S1.4.1 in%D1.6.1 load%B2.1.2 out%D1.6.1;

set in -32768, set load 1, tick, output; tock, output;
set in 1234, set load 0, tick, output; tock, output;
