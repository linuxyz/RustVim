#!/usr/bin/env python3
"""Regenerate registry_generated.rs from docs/inventory/inventory.json."""
import json, re
from pathlib import Path

inv = json.load(open('docs/inventory/inventory.json'))
ex_cmds = Path('/tmp/rvim-vim/vim-9.2.1338/src/ex_cmds.h')
command_meta = {}
def feature_for(name, handler=''):
    lowered = name.lower()
    prefixes = {
        'terminal': 'terminal',
        'term': 'terminal',
        'gui': 'gui',
        'menu': 'gui',
        'popup': 'popup',
        'job': 'job',
        'ch_': 'job',
        'channel': 'job',
        'vim9': 'vim9',
        'class': 'vim9',
        'def': 'vim9',
        'interface': 'vim9',
        'enum': 'vim9',
        'js_': 'quickjs',
        'quickjs': 'quickjs',
    }
    for prefix, feature in prefixes.items():
        if lowered.startswith(prefix) or handler.startswith(f'ex_' + prefix):
            return feature
    return 'core' 
function_meta = {}
builtin_doc = Path('/tmp/rvim-vim/vim-9.2.1338/runtime/doc/builtin.txt')
if builtin_doc.exists():
    for line in builtin_doc.read_text(errors='replace').splitlines():
        tag = line.find('*')
        if tag == -1 or '()' not in line:
            continue
        prefix = line[:tag].strip()
        if '(' not in prefix or not prefix.endswith(')'):
            continue
        name = prefix.split('(', 1)[0].strip()
        args = prefix[len(name) + 1 : -1]
        min_args = 0
        max_args = 0
        depth = 0
        index = 0
        while index < len(args):
            ch = args[index]
            if ch == '[':
                depth += 1
            elif ch == ']':
                depth = max(0, depth - 1)
            elif ch == '{' and depth == 0:
                min_args += 1
            elif ch == '.' and args[index : index + 3] == '...':
                max_args = -1
            index += 1
        if max_args != -1:
            max_args = args.count('{')
        function_meta[name] = (min_args, max_args)
if ex_cmds.exists():
    text = ex_cmds.read_text()
    for match in re.finditer(
        r'EXCMD\([^,]+,\s*"([^"]*)",\s*(\w+),\s*([^,]+),\s*(\w+)\)', text
    ):
        name, handler, flags, addr = match.groups()
        flags = flags.strip()
        min_args = 1 if 'EX_NEEDARG' in flags else 0
        max_args = -1 if any(
            flag in flags for flag in ('EX_EXTRA', 'EX_XFILE', 'EX_TRLBAR')
        ) else 0
        command_meta[name] = (handler, flags, addr, min_args, max_args)
reg = Path('crates/rvim-editor/src/registry.rs').read_text()
implemented = set(re.findall(r'cmd!\("([^"]+)"', reg))
events_src = Path('crates/rvim-editor/src/autocmd.rs').read_text()
event_impl = set(re.findall(r'=> Self::(\w+)', events_src))
name_block = events_src.split('pub fn name(&self) -> &str {')[1].split('    }')[0]
event_pairs = dict(re.findall(r'Self::(\w+) => "([^"]+)"', name_block))
event_names = {event_pairs.get(v, v) for v in event_impl}
builtins = set('''type typename string str2nr nr2str str2float float2nr printf len empty copy deepcopy add extend remove insert sort reverse join split keys values has_key get index count range map filter matchstr match matchend substitute stridx strridx strpart strcharpart strlen strchars tolower toupper repeat char2nr nr2char call function funcref exists has eval execute getline setline line getreg setreg min max abs round floor ceil sqrt pow fmod and or xor invert indent foldclosed foldclosedend foldlevel foldtext foldtextresult sign_define sign_undefine sign_getdefined sign_place sign_unplace sign_getplaced sign_jump sign_placelist sign_unplacelist prop_type_add prop_type_change prop_type_delete prop_type_get prop_type_list prop_add prop_add_list prop_clear prop_find prop_list prop_remove hlID hlexists synID synIDattr synIDtrans executable search system systemlist items win_gettype'''.split())

def lit(value):
    return json.dumps(value, ensure_ascii=False)

def table(name, rows):
    lines = [f'pub static {name}: &[NameSpec] = &[']
    for n, d, desc, impl, handler, flags, min_args, max_args, feature in rows:
        lines.append(
            f'    NameSpec {{ name: {lit(n)}, doc: {lit(d)}, description: {lit(desc)}, '
            f'implemented: {str(impl).lower()}, handler: {lit(handler)}, flags: {lit(flags)}, '
            f'min_args: {min_args}, max_args: {max_args}, feature: {lit(feature)} }},'
        )
    lines.append('];')
    return '\n'.join(lines)

cmds = []
for item in inv['commands']:
    name = item['name']
    handler, flags, _addr, min_args, max_args = command_meta.get(
        name, (name, '', '', 0, -1)
    )
    cmds.append((name, item.get('doc') or name, item.get('description') or '', name in implemented, handler, flags, min_args, max_args, feature_for(name, handler)))
funcs = []
for item in inv['functions']:
    name = item['name']
    min_args, max_args = function_meta.get(name, (0, -1))
    funcs.append((name, item.get('doc') or name, '', name in builtins, name, 'builtin' if name in builtins else '', min_args, max_args, feature_for(name)))
events = [(i['name'], i.get('doc') or i['name'], i.get('description') or '', i['name'] in event_names, i['name'], 'event', 0, 0, 'core') for i in inv['events']]
header = "//! Generated from docs/inventory/inventory.json. Do not edit by hand.\n//! Regenerate with tools/gen_registry.py.\n\npub struct NameSpec {\n    pub name: &'static str,\n    pub doc: &'static str,\n    pub description: &'static str,\n    pub implemented: bool,\n    pub handler: &'static str,\n    pub flags: &'static str,\n    pub min_args: i8,\n    pub max_args: i8,\n    pub feature: &'static str,\n}\n\n"
text = header + table('ALL_COMMANDS', cmds) + '\n\n' + table('ALL_FUNCTIONS', funcs) + '\n\n' + table('ALL_EVENTS', events) + '\n'
Path('crates/rvim-editor/src/registry_generated.rs').write_text(text)
print(len(cmds), len(funcs), len(events), len(implemented), len(event_names))
