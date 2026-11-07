# Contributing to Reclaimer

Thank you for helping! Bug reports, ideas and code are all welcome.

## Reporting a bug

[Open an issue](https://github.com/dincertekin/reclaimer/issues/new) and include:

- your system (for example *macOS 26 on a MacBook Air*, *Windows 11*, *Ubuntu 24.04*);
- what you scanned (an SD card, an iPhone 15, a disk image...);
- what happened, and what you expected;
- if a drive or phone doesn't show up, the output of `Reclaimer --diagnose`
  (or `cargo run -- --diagnose`). It lists what Reclaimer can see, and no
  file contents.

Please don't attach disk images or recovered files that could contain
personal data.

## Contributing code

1. Read [Developing Reclaimer](docs/development.md): how to run it, the
   project structure and the tests.
2. Keep changes focused, and add tests for new behaviour. Formats are tested
   against files made by independent tools, not by Reclaimer itself.
3. Run `cargo test` and `cargo clippy --all-targets`: both should be clean,
   with no warnings.
4. Open a pull request describing what changed and why.

The interface follows a few rules: black on white, one obvious action per
screen, plain words instead of jargon. If a change adds a button, it should
earn its place.
