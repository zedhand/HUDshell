# Contributing to HUDshell

Thanks for helping. HUDshell is small and deliberately narrow: a compact always-on-top PowerShell 7
terminal for Windows 11 that must run command-line tools (Claude Code, Codex CLI) exactly as a normal terminal does.
Read [docs/design.md](docs/design.md) and [docs/known-gaps.md](docs/known-gaps.md) before proposing larger changes.

## Build and test

```powershell
scripts\build-release.ps1                 # release exe, build-machine paths stripped
cargo build --features harness            # harness build used by the scenario suites
scripts\test-all.ps1 -Suite static        # unit tests and release-binary checks
```

Pull requests run the `static` suite on GitHub Actions. The other suites (`ui`, `clickfocus`, `clipboard`,
`hideshow`, `autotest`, `release`, `config`, `soak`, `real`) need a real interactive Windows session and send real
hotkeys, mouse input and clipboard text, so they are run by hand. Read [docs/testing.md](docs/testing.md) before running
them, and keep your hands off the keyboard and mouse while they run. Run what is relevant to your change and say in the
pull request which suites you ran and which you did not.

## Ground rules

- **Real PowerShell, real ConPTY.** Do not emulate the shell or alter what programs inside it see.
- **No F-keys** for default hotkeys (many keyboards lack them). Every hotkey needs at least one modifier.
- **US keyboard layout** is the supported layout for now; other layouts are listed in the known gaps.
- **Test code stays out of the product.** Scenarios and injectors live in `src/harness.rs` behind
  `--features harness`; `scripts/test-all.ps1` checks that a normal build contains none of it.
- **No input injection into other programs and no keyboard hooks.** Global keys use `RegisterHotKey` only.
- **Keep the UI local.** The page is embedded in the exe, served from memory, with no remote sources.

## Keep the record honest

When you test or implement something, update [docs/test-results.md](docs/test-results.md) and
[docs/known-gaps.md](docs/known-gaps.md) in the same change. Claim only what a test actually showed, and say plainly
what was not run.

## Repository hygiene

Never commit screenshots, recordings, logs, `config.json`, personal names, e-mail addresses or local paths.
Check your staged files before every commit. `.gitignore` covers common cases but is not a substitute for looking.
The test harness writes `hudshell-test.log` in the project root; delete it before committing.

## Pull requests

Keep changes focused, explain the user-visible effect, and match the existing code style. Run `cargo test` and
`cargo build --features harness` before opening one. By contributing you agree that your work is licensed under the
[MIT licence](LICENSE).

## Security

Please report vulnerabilities privately through GitHub's "Report a vulnerability" option on the Security tab rather
than in a public issue.
