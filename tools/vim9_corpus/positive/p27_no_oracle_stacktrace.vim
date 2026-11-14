vim9script
# vim9-corpus-no-oracle: v:stacktrace was added in Vim 9.2; the local oracle is Vim 9.1.
# The parser still has to accept the try/catch statement and the v: variable.
var caught = ''
try
  throw 'boom'
catch /boom/
  caught = 'caught'
  echomsg 'stacktrace=' .. string(type(v:stacktrace))
endtry
echomsg caught
