vim9script
# expect: fail
# Vim reports E171 (Missing :endif); exact E-codes for invalid programs are
# not a Phase 1 goal, so only rejection is asserted here.
if true
  echomsg 'inside'
