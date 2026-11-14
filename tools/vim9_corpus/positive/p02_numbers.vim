vim9script
# p02: number and float literals and their string conversion.
var decimal = 42
var negative = -7
var hex = 0x1f
var binary = 0b1010
var octal = 0o17
var big = 1234567890123
var f1 = 1.5
var f2 = 1.0e3
var f3 = 0.25
var f4 = -2.5
var f5 = 1.5e-3
echomsg 'dec=' .. decimal .. ' neg=' .. negative .. ' hex=' .. hex
echomsg 'bin=' .. binary .. ' oct=' .. octal .. ' big=' .. big
echomsg 'f1=' .. f1 .. ' f2=' .. f2 .. ' f3=' .. f3
echomsg 'f4=' .. f4 .. ' f5=' .. f5
