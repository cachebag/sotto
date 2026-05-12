# Contributing - WIP subject to change

Thank you for your interest in contributing to `sotto`. We welcome contributions from everyone. Please take a look at our [issues](https://github.com/cachebag/sotto/issues) for what we're currently working on.

## AI

I use AI, and there's a 99% chance anyone who contributes to this project will use AI as well. Please disclose if/how you used it in your PR.

One thing I will not tolerate are drive-by PRs that are not tied to an issue, major features or refactors or large diffs in general with no clear purpose or direction. I will just close the PR without explaining why.

## Code style

We have some pretty basic checks in our CI pipeline- `fmt`, `clippy`, and `test`. Please run these locally before submitting a pull request.

Beyond that, certain conventions will be enforced by a human, depending on the work you are doing.

## Commit hygiene

We use [conventional commits](https://www.conventionalcommits.org/en/v1.0.0/) for our commit messages. Please follow these guidelines when committing your changes.

Generally, we like to follow this format for PRs tied to issue:

```bash
$type(#issue-number): <short description>
```
Where type is one of:
- feat: A new feature
- fix: A bug fix
- chore: A chore (non-code change)
- docs: Documentation only changes
- refactor: A code change that neither fixes a bug nor adds a feature
- test: Adding missing tests or correcting existing tests
- perf: A code change that improves performance
- style: Changes that do not affect the meaning of the code (white-space, formatting, missing semi-colons, etc)

For larger changes, a detailed summary in the commit is encouraged.

```bash
$type(#issue-number): <short description>

<detailed description>
```

## License 

By contributing to this project, you agree to the terms of the [Apache 2.0 License](LICENSE-APACHE) or the [MIT License](LICENSE-MIT).

Any of your changes will be dual-licensed under the terms of the Apache 2.0 License and the MIT License with no additional terms or conditions.