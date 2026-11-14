vim9script
# p01: the vim9script header and comments (leading, trailing, in a def body).
#{{{ fold marker comment
var answer = 41
var next = answer + 1  # trailing comment after a statement
#}}}
def Describe(): string
  # comment inside a def body
  return 'answer=' .. answer
enddef
echomsg Describe()
echomsg 'next=' .. next
