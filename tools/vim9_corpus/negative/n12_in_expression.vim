vim9script
# expect: fail
# Vim9 script has no "in" expression operator (use index()/has_key();
# ":for x in list" is separate).  Vim rejects this with E488.
var list = [1, 2]
var found = 1 in list
