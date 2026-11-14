# Vim9 differential-testing harness

tools/vim9_diff.py drives the Rust Vim9 front end against a real Vim oracle and keeps its acceptance corpus in tools/vim9_corpus/.  Stdlib only; run from the repository root.

## Layout
    positive/*.vim (27 cases)   negative/*.vim (12 cases)
    support/*.vim (3 import modules)   expected/*.out (Vim output per oracle-checked positive)

## Conventions
Positives start with a 'vim9script' header, end with top-level :echomsg assertions and have no file/shell/job side effects.  Negatives carry '# expect-error: E-code' on line 2, or '# expect: fail' when only rejection is asserted (exact E-codes are not a Phase 1 goal; comments record Vim's real code).
A positive may use '# vim9-corpus-no-oracle: <reason>' instead: it is still parse-checked but has no expected/ file (only p27, which needs Vim 9.2).

## Commands
    python3 tools/vim9_diff.py check-corpus    # name/header/marker/expected hygiene
    python3 tools/vim9_diff.py oracle          # capture expected/*.out from Vim
    python3 tools/vim9_diff.py oracle --check  # re-capture and compare, no writes
    python3 tools/vim9_diff.py parse [--filter SUBSTR] [--verbose] [--json OUT]
    python3 tools/vim9_diff.py diff            # Phase 2 stub, exits 2 by design

Exit codes: 0 success, 1 case/validation failure, 2 missing dependency (Vim, cargo, the vim9_parse example) or diff unimplemented.  parse invokes 'cargo run -q -p rvim-viml --example vim9_parse -- FILE': positives must print 'OK' and exit 0, negatives must exit nonzero and contain the expected E-code.
Without the example parse reports that and marks all cases unrun, never passed; a negative rejected only with an 'unimplemented' parser error is SKIPped, not passed, and a trailing 'OK items=27' debug suffix is tolerated and shown in the table.

## Oracle invocation
    vim -Nu NONE -n -es -i NONE --not-a-term -c 'set encoding=utf-8' -c 'set nomore' \
        -c 'redir => g:vim9_corpus_output' -c 'source <absolute path>' -c 'redir END' \
        -c 'call writefile(split(g:vim9_corpus_output, "\n"), "<temp out>")' -c 'qa!'

For each oracle-checked positive, with stdin /dev/null.  :redir captures :echo/:echomsg and error text exactly; the file is stored byte-for-byte in expected/.  Vim must exit 0, write the file, and emit no 'E-code:' or 'Error detected' line.

## Adding a case
Add positive/<name>.vim, run 'oracle', then check-corpus and oracle --check.  If local Vim rejects valid 9.2 syntax, mark '# vim9-corpus-no-oracle: reason' rather than weakening the case.  Negatives need a new name plus '# expect-error: E-code' or '# expect: fail'.

## Known limitations
The oracle is Vim 9.1 (patches 1-1752), older than the 9.2.1338 target; it already accepts nested :def, generics, classes, interfaces, enums and type aliases (only v:stacktrace in p27 is no-oracle).
Vim 9 has no 'in' expression operator (use index()/has_key(); ':for x in list' is separate), asserted by n12; funcref variables need a capital first letter (E704), see p21.  diff is a Phase 2 stub that loads expected/ and exits 2, never faking a pass.
