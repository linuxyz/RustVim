vim9script
# p07: destructuring declarations and for-loop destructuring.
var pair = [10, 20]
var [first, second] = pair
var [head, middle; rest] = [1, 2, 3, 4]
var [a: number, b: number] = [7, 8]
var dict = {one: 1, two: 2}
for [key, value] in dict->items()
  echomsg 'item=' .. key .. ':' .. value
endfor
echomsg 'pair=' .. first .. ',' .. second
echomsg 'head=' .. head .. ' middle=' .. middle .. ' rest=' .. string(rest)
echomsg 'typed=' .. a .. ',' .. b
