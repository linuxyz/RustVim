vim9script
# p15: -> method chains, including line continuation and lambdas.
var words = ['beta', 'alpha', 'gamma']
echomsg 'sorted=' .. words->sort()->join(',')
echomsg 'mapped=' .. ['a', 'b']->map((_, v) => v->toupper())->join('')
echomsg 'trim=' .. '  x  '->trim()->toupper()
var numbers = [1, 2, 3, 4]
echomsg 'filtered=' .. string(numbers->filter((_, v) => v % 2 == 0))
echomsg 'reduced=' .. numbers->reduce((acc, v) => acc + v)
var chained = 'hello world'
  ->split(' ')
  ->map((_, v) => v->toupper())
  ->join('-')
echomsg 'chained=' .. chained
