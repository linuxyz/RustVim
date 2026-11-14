vim9script
# expect: fail
# A class body must be closed before executable script lines; the echomsg
# is not a valid class member (Vim reports E1318), so no exact code here.
class Incomplete
  public var value: number = 1
echomsg 'after-class'
