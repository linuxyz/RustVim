vim9script
# p03: single/double quoted strings, escapes and interpolated strings.
var single = 'single ''quoted'' text'
var escaped = "double\twith\tescape"
var plain = "backslash: \\ end"
var name = 'world'
var interp = $"hello {name}"
var math = $"one plus two is {1 + 2}"
var single_interp = $'raw {name}'
var upper = 'abc'->toupper()
var joined = ['a', 'b']->join('-')
echomsg 'single=' .. single
echomsg 'escaped=' .. strtrans(escaped)
echomsg 'plain=' .. plain
echomsg 'interp=' .. interp
echomsg 'math=' .. math
echomsg 'single_interp=' .. single_interp
echomsg 'upper=' .. upper .. ' joined=' .. joined
