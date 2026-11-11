# Contributing

Thanks for helping. Den is small on purpose, so a short issue before a large change saves
everyone time.

## Ground rules

Den is read-only. Please read [AGENTS.md](AGENTS.md) before changing behaviour: it lists
the fixed commands Den may run, what each area may read, and what must stay withheld.
Changes that add a command, a network call or a write need discussion first.

## Workflow

```sh
just check    # format, Clippy, tests, release build
just smoke    # offline terminal checks in a real PTY
```

Without `just`, see [development](docs/DEVELOPMENT.md) for the equivalent commands.

- Add or update a synthetic fixture under `fixtures/` and a test under `tests/` for any
  visible change. Mark values that must never be shown with `PRIVATE` and assert they
  stay off screen.
- Never commit captures or fixtures taken from a real machine.
- Keep [the keyboard guide](docs/KEYBOARD.md) in step with behaviour.
- Use short, imperative commit subjects, such as `Show uv tools in Packages`.

By contributing, you agree that your work is licensed as the project is: MIT or
Apache-2.0, at the user's option.

## Security

Report vulnerabilities privately; see [SECURITY.md](SECURITY.md).
