vim9script
# p26: ProgressBar pattern adapted from runtime/syntax/2html.vim: a class
# with public and private members, nested def in a method and lambda map.
class ProgressBar
  public var title: string
  public var max_value: number
  public var cur_value: number = 0
  public var last_value: number = 0
  public var needs_redraw: number = 0
  public var pb_len: number = 0
  public var max_len: number = 0
  public var progress_ticks: list<number> = []
  var subtractedlen: number

  def new(title: string, max_value: number)
    this.title = title
    this.max_value = max_value
    this.subtractedlen = len(split(this.title, '\zs')) + 3 + 4 + 2 + 2
  enddef

  def Paint()
    var max_len = 40
    var pb_len = 0
    var cur_value: number

    def CalculateTicks(len: number)
      if len <= 0
        pb_len = 100
      else
        pb_len = len
      endif
      this.progress_ticks = range(pb_len + 1)->map((_, v) => v * this.max_value / pb_len)
    enddef

    if max_len != this.max_len
      this.max_len = max_len
      pb_len = max_len - this.subtractedlen
      CalculateTicks(pb_len)
      this.needs_redraw = 1
      this.pb_len = pb_len
    else
      cur_value = this.last_value
      pb_len = this.pb_len
    endif

    var cur_val_max = pb_len > 0 ? pb_len : 100
    while cur_value < cur_val_max && this.cur_value > this.progress_ticks[cur_value]
      cur_value += 1
    endwhile

    if this.last_value != cur_value || this.needs_redraw
      this.needs_redraw = 1
      this.last_value = cur_value
      echomsg 'paint ' .. this.title .. ' ticks=' .. len(this.progress_ticks)
        .. ' cur=' .. cur_value
    endif
  enddef

  def Incr(delta: number = 1)
    this.cur_value += delta
    this.cur_value = (this.cur_value > this.max_value ? this.max_value : this.cur_value)
    this.Paint()
  enddef
endclass
var bar = ProgressBar.new('Progress', 10)
bar.Incr(3)
bar.Incr(7)
echomsg 'final=' .. bar.cur_value
