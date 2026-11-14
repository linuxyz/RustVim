vim9script
# expect: fail
# Vim reports E1009 (Missing > after type) for the unterminated generic type.
var numbers: list<number
