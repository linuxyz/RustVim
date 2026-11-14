vim9script
# p04: blob literals.
var empty = 0z
var raw = 0zDEADBEEF
var dotted = 0z01.02.03
var text = 0z68656C6C6F
echomsg 'empty=' .. string(empty) .. ' len=' .. len(empty)
echomsg 'raw=' .. string(raw) .. ' len=' .. len(raw)
echomsg 'dotted=' .. string(dotted) .. ' len=' .. len(dotted)
echomsg 'text=' .. string(text)
