vim9script
# p10: def parameters: typed, default, varargs, any, void return.
def Add(a: number, b: number): number
  return a + b
enddef
def Greet(name: string = 'world'): string
  return 'hello ' .. name
enddef
def Sum(...values: list<number>): number
  var total = 0
  for value in values
    total += value
  endfor
  return total
enddef
def Ignore(..._): string
  return 'ignored'
enddef
def Show(value: any): string
  return string(value)
enddef
def Noop(): void
enddef
Noop()
echomsg 'add=' .. Add(2, 3)
echomsg 'greet=' .. Greet() .. ' named=' .. Greet('vim')
echomsg 'sum=' .. Sum() .. ' sum3=' .. Sum(1, 2, 3)
echomsg 'ignore=' .. Ignore(1, 2)
echomsg 'show=' .. Show([1, 2])
