vim9script
# p20: enums, their name/ordinal properties and enum constructors.
enum Color
  Red,
  Green,
  Blue
endenum
enum Planet
  Earth(1, false),
  Mars(2, false),
  Jupiter(95, true)
  var moons: number
  var rings: bool
  def new(this.moons, this.rings)
  enddef
endenum
var color = Color.Green
echomsg 'name=' .. color.name .. ' ordinal=' .. color.ordinal
echomsg 'red=' .. Color.Red.name .. ' blue=' .. Color.Blue.ordinal
echomsg 'earth=' .. Planet.Earth.moons
echomsg 'jupiter=' .. Planet.Jupiter.moons .. ' rings=' .. Planet.Jupiter.rings
