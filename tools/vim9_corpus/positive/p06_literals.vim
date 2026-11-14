vim9script
# p06: list/dict/tuple literals, including literal and computed dict keys.
var list = [1, 2, 3]
var empty_list: list<string> = []
var dict = {a: 1, b: 2}
var quoted = {'x': 1, "y": 2}
var key = 'k'
var computed = {[key]: 5}
var numeric = {1: 'one'}
var tuple = (1, 'two')
var single_tuple = (9,)
echomsg 'list=' .. string(list) .. ' empty_list=' .. string(empty_list)
echomsg 'dict=' .. string(dict) .. ' quoted=' .. string(quoted)
echomsg 'computed=' .. string(computed) .. ' numeric=' .. string(numeric)
echomsg 'tuple=' .. string(tuple) .. ' single=' .. string(single_tuple)
