vim9script
# p09: try/catch/finally, throw and v:exception.
var message = ''
try
  throw 'boom'
catch /boom/
  message = 'caught-boom: ' .. v:exception
catch
  message = 'caught-other'
finally
  echomsg 'finally-ran'
endtry
echomsg message
def Thrower(): number
  try
    throw 'inner'
  catch /inner/
    return 7
  finally
    echomsg 'inner-finally'
  endtry
  return 0
enddef
echomsg 'thrower=' .. Thrower()
