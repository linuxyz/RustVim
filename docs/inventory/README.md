# Generated Vim inventories

Source: **Vim 9.2.1338**  
Generated: 2026-11-13 by tools/gen_inventory.py.

These files are mechanically extracted from the unmodified Vim runtime tree.
They make "complete" auditable and support per-item coverage tracking. Test
fixtures under <code>testdir/</code> are excluded; they are not loaded by the
editor. The curated, selectable feature list is [../FEATURES.md](../FEATURES.md).

| Inventory | Count | File |
|---|---:|---|
| Ex commands | 593 | [commands.md](commands.md) |
| Mode keys (insert/normal/visual/cmdline/terminal) | 695 | [keys.md](keys.md) |
| Options | 455 | [options.md](options.md) |
| Built-in functions | 605 | [functions.md](functions.md) |
| Autocommand events | 136 | [events.md](events.md) |
| has() features | 211 | [features.md](features.md) |
| Runtime surface | -- | [runtime-surface.md](runtime-surface.md) |

Machine-readable form: [inventory.json](inventory.json).

The terminal emulator (<code>:terminal</code>, <code>term_*</code> functions and
Terminal-Job mode) is descoped by project decision. Its commands, functions,
options and keys remain in these mechanical inventories so the gap is visible
and auditable; they are not part of the active catalog.

## Regenerate

    python3 tools/gen_inventory.py --runtime /path/to/vim/runtime \
      --label "Vim 9.2.1338" --out docs/inventory

The checked-in snapshot is generated from the pinned runtime named in
[../RUNTIME-COMPAT.md](../RUNTIME-COMPAT.md); never edit generated files by hand.
Review the diff. New commands/options/functions/events need a catalog decision:
in scope, deferred, or excluded.
