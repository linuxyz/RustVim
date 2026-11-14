vim9script
# p23: export const/var/final/def/class/type/enum from a Vim9 script.
export const VERSION = 7
export var counter: number = 0
export final fixed = 'fixed'
export def Bump(): number
  counter += 1
  return counter
enddef
export class Box
  public var value: number
  def new(value: number)
    this.value = value
  enddef
endclass
export type Numbers = list<number>
export enum Mode
  Off,
  On
endenum
echomsg 'version=' .. VERSION
echomsg 'bump=' .. Bump() .. ',' .. Bump()
var box = Box.new(9)
echomsg 'box=' .. box.value
var numbers: Numbers = [1, 2]
echomsg 'numbers=' .. string(numbers)
echomsg 'mode=' .. Mode.On.name
