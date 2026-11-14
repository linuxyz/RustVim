vim9script
# p08: if/elseif/else, while, for, continue and break.
var value = 3
if value == 1
  echomsg 'one'
elseif value == 2
  echomsg 'two'
else
  echomsg 'other'
endif
var total = 0
var n = 0
while n < 5
  n += 1
  if n == 3
    continue
  endif
  total += n
endwhile
for item in [10, 20, 30]
  total += item
endfor
for ch in 'abc'
  echomsg 'char=' .. ch
endfor
for i in range(3)
  if i == 2
    break
  endif
  echomsg 'i=' .. i
endfor
echomsg 'total=' .. total .. ' n=' .. n
