vim9script
# p24: ordinary Ex commands mixed with Vim9 code (execute, set, normal!).
var message = 'from-execute'
execute 'echomsg "' .. message .. '"'
set tabstop=4
set shiftwidth=4
echomsg 'tabstop=' .. &tabstop .. ' shiftwidth=' .. &shiftwidth
normal! ihello
echomsg 'line=' .. getline(1)
normal! A world
echomsg 'line2=' .. getline(1)
def InFunction(): string
  execute 'echomsg "exec-in-def"'
  set number
  normal! osecond
  return getline(2) .. ' number=' .. &number
enddef
echomsg 'in-function=' .. InFunction()
set nonumber
