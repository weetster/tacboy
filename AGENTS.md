# tacboy Agent Instructions

Follow [CLAUDE.md](CLAUDE.md) for Tacboy-specific project guidance,
Tacit source editing rules, host binding workflow, gotchas, and hand-off
checks.

## Ticket workflow

Use the `gh` CLI for GitHub interactions whenever possible. When reading
issues, commenting, closing tickets, or otherwise working with GitHub from
this repository, prefer `gh` over direct API calls or the web UI.

When the user asks to handle a GitHub issue or ticket, treat that as an
end-to-end workflow unless they explicitly narrow the scope:

1. Read the ticket first.
2. Implement the fix in the repository.
3. Add or update tests when the change needs coverage.
4. Run the relevant verification for the change.
5. Commit the resulting changes.
6. Push the commit.
7. Close the ticket with a comment summarizing what was done.
