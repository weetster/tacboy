# tacboy Agent Instructions

This is a Tacit project pinned by `tacit-toolchain.toml`.

## Reading the language and workflow contracts

- Run `tacit primer` to print the Tacit-Lite language primer that matches this toolchain. Read it before writing or editing source. Do not copy primer prose from another repository or another toolchain version.
- The agent workflow companion is installed at `share/tacit/workflow/agent-workflow.md` in the toolchain prefix. Read it before running tools.

## Editing source

Do not hand-edit `.tac` files. The `.tac` format is canonical S-expression bytes with BLAKE3 definition-hash references; the primer teaches the authoring view (`.taca`), which is a different surface syntax. The two do not line up token for token, and `.tac` hashes change with every edit. Edit via the round-trip loop instead:

1. Render existing source as authoring view to a scratch path outside the project, for example `tacit render src/main.tac --as authoring -o /tmp/tacboy2.taca`.
2. Edit the scratch `.taca` using the authoring-view syntax from the primer.
3. Canonicalize back into the project: `tacit canonicalize /tmp/tacboy2.taca -o src/main.tac --force`. That rewrites both `src/main.tac` and `src/main.tacd`.
4. Delete the scratch `.taca`. Do not check `.taca` files into this project.

After any source edit, definition hashes change. Run `tacit lock` to refresh `tacit.lock`, then update any `[exports]`, `[bin]`, or `[[tests]]` entries in `tacit.toml` that reference the old hashes. Use `tacit view src --as inspection --hashes` (or read `tacit.lock`) to look up the new ones.

## Hand-off

Before handing off changes, run `tacit lock`, `tacit check .`, and `tacit test . --format json` when LLVM support is available.
