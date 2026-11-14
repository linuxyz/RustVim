vim9script
# p11: def return types (containers, float, blob, bool) and recursion.
def Numbers(): list<number>
  return [1, 2, 3]
enddef
def Mapping(): dict<string>
  return {a: 'x'}
enddef
def Pair(): tuple<number, string>
  return (1, 'one')
enddef
def Ratio(): float
  return 2.5
enddef
def Data(): blob
  return 0z0102
enddef
def Choice(flag: bool): string
  return flag ? 'yes' : 'no'
enddef
def Factorial(n: number): number
  return n <= 1 ? 1 : n * Factorial(n - 1)
enddef
echomsg 'numbers=' .. string(Numbers())
echomsg 'mapping=' .. string(Mapping())
echomsg 'pair=' .. string(Pair()) .. ' ratio=' .. Ratio()
echomsg 'data=' .. string(Data())
echomsg 'choice=' .. Choice(true) .. ',' .. Choice(false)
echomsg 'factorial=' .. Factorial(5)
