# Contributing

Open an issue describing the user-visible problem before a large change. Small
fixes can be proposed directly. Keep application text in English and preserve the
light UI, Word/Excel-only scope, native save prompts and user-controlled settings.

1. Follow README.md to set up the Windows toolchain.
2. Make a focused change and add a regression test for behavior that can be tested.
3. Run `cargo fmt --check`, `build.ps1` and the audits listed in README.md.
4. Describe what changed, why, checks performed and live Office checks not performed.

Never suppress Office save alerts, mark documents saved to discard changes, modify
ExtendOffice files, bypass activation, or include proprietary binaries/resources.
Do not install or manipulate a contributor's Office documents as part of tests.

Do not commit logs, user documents, credentials, generated binaries, personal paths
or build caches. Keep all vendored third-party license notices and checksums.
Contributions are submitted under this repository's MIT License. Do not include
material you lack permission to contribute. Be respectful and keep reviews focused
on the work. No signing certificate or private credential is required to contribute.
