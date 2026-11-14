vim9script
# p13: generic def functions and a nested generic function.
def First<T>(values: list<T>): T
  return values[0]
enddef
def Pair<T, U>(first: T, second: U): tuple<T, U>
  return (first, second)
enddef
def Identity<T>(value: T): T
  return value
enddef
def Outer(): string
  def Inner<T>(value: T): T
    return value
  enddef
  return string(Inner<number>(7)) .. ',' .. Inner<string>('s')
enddef
echomsg 'first=' .. First<number>([10, 20])
echomsg 'pair=' .. string(Pair<number, string>(1, 'x'))
echomsg 'identity=' .. Identity<string>('id') .. ',' .. Identity<number>(3)
echomsg 'nested=' .. Outer()
