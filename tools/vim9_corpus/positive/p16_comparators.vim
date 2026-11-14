vim9script
# p16: comparisons: ==, !=, <, >=, =~, !~, ==#, ==?, is, isnot, &&, !.
var a = 3
var b = 3
var text1 = 'abc'
var text2 = 'abd'
var list1 = [1, 2]
var list2 = [1, 2]
echomsg 'eq=' .. (a == b) .. ' ne=' .. (a != b)
echomsg 'lt=' .. (text1 < text2) .. ' ge=' .. (b >= a)
echomsg 'match=' .. ('abc123' =~ '\d\+') .. ' nomatch=' .. ('abc' !~ '\d')
echomsg 'case=' .. ('ABC' ==# 'abc') .. ',' .. ('ABC' ==? 'abc')
echomsg 'listeq=' .. (list1 == list2) .. ' listis=' .. (list1 is list2)
echomsg 'same=' .. (list1 is list1) .. ' isnot=' .. (list1 isnot list2)
echomsg 'bool=' .. (true && !false)
