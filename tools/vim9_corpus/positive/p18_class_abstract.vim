vim9script
# p18: abstract class, abstract method, inheritance and list<Shape>.
abstract class Shape
  public var color: string = 'black'
  abstract def Area(): number
  def Describe(): string
    return this.color .. ' shape area ' .. this.Area()
  enddef
endclass
class Square extends Shape
  public var side: number
  def new(side: number)
    this.side = side
  enddef
  def Area(): number
    return this.side * this.side
  enddef
endclass
class Rect extends Shape
  public var width: number
  public var height: number
  def new(width: number, height: number)
    this.width = width
    this.height = height
  enddef
  def Area(): number
    return this.width * this.height
  enddef
endclass
var square = Square.new(3)
var rect = Rect.new(2, 5)
echomsg 'square=' .. square.Describe()
echomsg 'rect=' .. rect.Describe()
var shapes: list<Shape> = [square, rect]
var total = 0
for shape in shapes
  total += shape.Area()
endfor
echomsg 'total=' .. total
