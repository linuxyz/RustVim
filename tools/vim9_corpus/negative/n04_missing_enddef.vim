vim9script
# expect: fail
# Vim reports E1057 (Missing :enddef); only rejection is asserted.
def MissingEnd(): number
  return 1
