vim9script
# expect: fail
# Vim reports E1004 (white space required around '='); only rejection is
# asserted because the parser does not promise Vim's exact diagnostic codes.
var x=1
