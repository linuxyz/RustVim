vim9script
# p19: interfaces, implements, interface types and multiple interfaces.
interface HasSurface
  def Surface(): number
endinterface
interface Named
  def GetName(): string
endinterface
class Square implements HasSurface
  public var size: number
  def new(size: number)
    this.size = size
  enddef
  def Surface(): number
    return this.size * this.size
  enddef
endclass
class Circle implements HasSurface, Named
  public var radius: number
  def new(radius: number)
    this.radius = radius
  enddef
  def Surface(): number
    return 3 * this.radius * this.radius
  enddef
  def GetName(): string
    return 'circle'
  enddef
endclass
var shapes: list<HasSurface> = [Square.new(4), Circle.new(2)]
var total = 0
for shape in shapes
  total += shape.Surface()
endfor
echomsg 'square=' .. Square.new(3).Surface()
echomsg 'total=' .. total
var named: Named = Circle.new(1)
echomsg 'named=' .. named.GetName()
