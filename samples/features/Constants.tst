load Constants.hdl,
output-file Constants.out,
compare-to Constants.cmp,
output-list in%B1.8.1 masked%B1.8.1 ones%X1.4.1 low copy%D1.4.1;

set in 0, eval, output;
set in %B11111111, eval, output;
set in %B10101010, eval, output;
