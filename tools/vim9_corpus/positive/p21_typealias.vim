vim9script
# p21: type aliases for tuples, lists, dicts and func types, including the
# optional (?type) and varargs (...list<type>) func-type argument forms.
# Vim 9 requires funcref-typed variables to start with a capital letter (E704).
type Pair = tuple<number, string>
type Numbers = list<number>
type Lookup = dict<string>
type Doubler = func(number): number
type OptionalCallback = func(?number): number
type Summer = func(...list<number>): number
type Handler = func(string): void
var pair: Pair = (1, 'one')
var numbers: Numbers = [1, 2, 3]
var lookup: Lookup = {a: 'x'}
def Scale(n: number = 5): number
  return n * 2
enddef
def SumAll(...values: list<number>): number
  return values->reduce((acc, v) => acc + v)
enddef
def Notify(_message: string): void
enddef
var Double: Doubler = (n: number): number => n * 2
var Scaled: OptionalCallback = Scale
var Sum: Summer = SumAll
var HandlerRef: Handler = Notify
echomsg 'pair=' .. string(pair)
echomsg 'numbers=' .. string(numbers) .. ' lookup=' .. string(lookup)
echomsg 'double=' .. Double(4)
echomsg 'optional=' .. Scaled() .. ',' .. Scaled(3)
echomsg 'summer=' .. Sum(1, 2, 3)
HandlerRef('ignored')
echomsg 'handler=assigned'
