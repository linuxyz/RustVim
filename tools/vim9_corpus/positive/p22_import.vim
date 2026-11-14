vim9script
# p22: import with an alias, import an autoload module with an alias.
import './../support/greet.vim' as greet
import './../support/consts.vim' as consts
import autoload './../support/auto.vim' as auto
echomsg 'greet=' .. greet.Greet('vim')
echomsg 'answer=' .. consts.ANSWER
echomsg 'counter=' .. consts.COUNTER
echomsg 'auto=' .. auto.AutoHello()
