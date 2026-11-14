vim9script
# expect: fail
# Vim reports E990 (Missing end marker) for the unterminated heredoc.
var lines =<< trim END
  alpha
