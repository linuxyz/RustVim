vim9script
# expect: fail
# Vim reports E170 (Missing :endfor).
for i in range(3)
  echomsg i
