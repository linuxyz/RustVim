vim9script
# p05: var/final/const with and without explicit types.
var untyped = 1
var typed: number = 2
var text: string = 'three'
final fixed = 'fixed'
final fixed_typed: string = 'fixed2'
const C1 = 10
const C2: number = 20
var flag: bool = true
var nothing = null
echomsg 'untyped=' .. untyped .. ' typed=' .. typed .. ' text=' .. text
echomsg 'fixed=' .. fixed .. ' fixed_typed=' .. fixed_typed
echomsg 'C1=' .. C1 .. ' C2=' .. C2 .. ' flag=' .. flag
echomsg 'nothing=' .. string(nothing)
