# <p align="center"> sotto

**<p align="center"><em>sotto voce</em> — “under the voice” or quietly, and under ones breath </p>**

https://github.com/user-attachments/assets/93632909-0fc1-4928-ab7a-f5a433d2c017

<p align="center"> <code>sotto</code> watches your git repo, sends diffs to an LLM API (e.g. OpenRouter), and surfaces a <b>cached commit message</b> in your shell when you commit.

## Install

Nothing interesting here yet but I will set something up soon. Just clone the repo and run `cargo install --path .` and then `sotto setup`

## Current state
I've only got a prototype working for the zsh side, and it is deliberately thin. Control is relenquished at the right time to allow for the cache entry to be surfaced. I just write a ([very hacky](https://github.com/cachebag/sotto/blob/master/src/shell/sotto.zsh)) widget into your zsh config.

Biggest problems now are implementing an ipc. Right now it's just the filesytem, technically. Eventually, I'd like to setup a unix socket that can get us things like realtime status changes, on demand generation and the ability to covenant over multiple repos on a machine (that is, in a better fashion than simply spawning a `sotto` process against individual repos).

Some other thoughts that aren't fully fleshed out in my head yet.

Fish, bash, etc. is all currently in development.

## Motivation

While I think [the leading](https://github.com/nutlope/aicommits) project in this space works _okay_, I still do not enjoy the friction around getting a commit into my repo. There is no reason I should be placed in a new prompt or session to commit my changes. It's too much ceremony for something that should be mindless.

This led me to think about what the best way to approach this problem is. Because as interesting as `aicommits` is, it simply _still_ does not solve the problem at hand: finding the right commit message for your changes as quickly as I did before AI existed. While I am no fan of AI, if it can save me whatever amount of time I spend over the course of years, thinking of and writing a commit message, that would be a win.

So over the weekend, I came up with `sotto`. No one actually cares about the generation of the commit message and you don't actually need any visual indicator at all that a commit message is being generated. It should just be there. And if you don't like the message, simply don't use it. 

The work is instead offloaded to _how_ we present this to the user. When I stage my changes and begin writing `git commit -m ...` I do not want to have to now wait for an LLM to read my diff and think of a message. I just want the message to be there. So let's just do all the busy work in the back. Let's keep the LLM on standy until a user signals that they are about to commit and push some work. 

I intentionally want `sotto` to live completely unbeknownst to me or my machine. It should only exist when we need it to, i.e., when we stage/commit our changes. So that's what it does and what it will do better, as I continue working over the concept better.

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
