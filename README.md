# <p align="center"> sotto

**<p align="center"><em>sotto voce</em> — “under the voice” or quietly, and under ones breath </p>**

**`sotto`** watches your git repo, sends diffs to an LLM API (e.g. OpenRouter), and surfaces a **cached conventional commit message** in your shell when you commit.

The idea of `sotto` is built around keeping the slow work out of the line editor. While I think [the leading](https://github.com/nutlope/aicommits) project in this space works well, I still do not enjoy the friction around getting a commit into my repo. 

So over the weekend, I wrote up this simple project. A small daemon that watches your working tree and the index, waits for your edits to settle, then turns the current diff into a single conventional commit line and stores it with a fingerprint of that diff. 
When you’re actually at `git commit`, the shell doesn’t call the network _at all_, it just asks whether the repo still looks like the snapshot that produced the cache. 

When something is staged, that snapshot is defined by what’s **staged**; otherwise it follows what’s 
still in the working tree, which is the same mental model as git itself.

I intentionally want `sotto` to live completely unbeknownst to me or my machine. It should only exist when we need it to, i.e., when we stage/commit our changes.

## Current state
The zsh side is deliberately thin. Control is relenquished at the right time to allow for the cache entry to be surfaced. A widget is written to your zsh config.

Fish, bash, etc. is all currently in development.

## Contributing

PRs and issues are welcome. I don't have a good set of guidelines right now. 

For non-trivial changes, opening an issue first helps align on direction. Keep commits focused, please. I am going to close vibe coded slop. AI is fine, just 
use it competently and disclose if/how you used it.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0), or  
- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project shall be dual-licensed as above, without any additional terms or conditions.
