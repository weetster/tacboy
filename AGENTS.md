# tacboy Agent Instructions

Follow [CLAUDE.md](CLAUDE.md) for Tacboy-specific project guidance,
Tacit source editing rules, host binding workflow, gotchas, and hand-off
checks.

## Ticket workflow

When the user asks to handle a GitHub issue or ticket, treat that as an
end-to-end workflow unless they explicitly narrow the scope:

1. Read the ticket first.
2. Implement the fix in the repository.
3. Add or update tests when the change needs coverage.
4. Run the relevant verification for the change.
5. Commit the resulting changes.
6. Push the commit.
7. Close the ticket with a comment summarizing what was done.
