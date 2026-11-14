vim9script
# p17: class with public/private (default) and static members, new() and
# methods.  Class variables are private unless marked "public".
class Counter
  public var count: number = 0
  public var name: string
  var step: number = 1
  static var instances: number = 0

  def new(name: string, step: number = 1)
    this.name = name
    this.step = step
    Counter.instances += 1
  enddef

  def Incr()
    this.count += this.step
  enddef

  def Get(): number
    return this.count
  enddef

  static def Total(): number
    return Counter.instances
  enddef
endclass
var counter = Counter.new('first')
counter.Incr()
counter.Incr()
echomsg 'count=' .. counter.Get() .. ' name=' .. counter.name
echomsg 'total=' .. Counter.Total()
var other = Counter.new('second', 5)
other.Incr()
echomsg 'other=' .. other.Get() .. ' total=' .. Counter.Total()
