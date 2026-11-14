vim9script
# p12: def nested inside def (Vim 9.2 language feature, also accepted by the
# local patched Vim 9.1 oracle) and closure access to outer locals.
def Outer(value: number): number
  def Inner(x: number): number
    return x * 2
  enddef
  return Inner(value) + 1
enddef
def Counter(): number
  var count = 0
  def Bump(): number
    count += 1
    return count
  enddef
  Bump()
  Bump()
  return Bump()
enddef
echomsg 'outer=' .. Outer(5)
echomsg 'counter=' .. Counter()
