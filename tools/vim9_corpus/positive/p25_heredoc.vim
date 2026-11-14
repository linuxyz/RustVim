vim9script
# p25: heredoc assignments: plain, trim, trim eval and executed lines.
var plain =<< END
raw line one
raw line two
END
var trimmed =<< trim END2
  alpha
  beta
END2
var evaluated =<< trim eval END3
  value is {1 + 2}
END3
var code_lines =<< trim END4
  echomsg 'executed-from-heredoc'
END4
execute code_lines[0]
echomsg 'plain=' .. plain->join('|')
echomsg 'trimmed=' .. trimmed->join('|')
echomsg 'evaluated=' .. evaluated[0]
