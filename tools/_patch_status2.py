import pathlib
p = pathlib.Path("docs/STATUS.md")
s = p.read_text()
old = "- **Quickfix:** list model, :cfile, :copen/:cclose/:cnext, make/grep ingestion, Vim-style errorformat including multi-line %A/%C/%Z groups."
new = old + "\n- **Substitute:** g/i/I/e/n/p/#/l/count/& flags, count-selected matches, :s repeat, :&, ~ replacement, \\U/\\L/\\u/\\l case regions.\n- **Editing:** replace mode overwrites text; dot repeats recorded insert/operator key sequences; visual block d/y/c/case operations.\n- **Ex modifiers:** :silent/:silent!, :noautocmd, :keepjumps, :tab, :vertical; global-local :set/:setlocal/:setglobal values; :setf/:setfiletype.\n- **Shell:** shell/shellcmdflag/shellxquote honored, range filters (:1,3!cmd), v:shell_error, :redir to files/registers/variables."
assert old in s
s = s.replace(old, new, 1)
s = s.replace("- Multi-row statusline and richer tabline item actions.\n", "")
p.write_text(s)
print("updated")
