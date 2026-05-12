# <p align="center"> sotto

https://github.com/user-attachments/assets/93632909-0fc1-4928-ab7a-f5a433d2c017

<p align="center"> <code>sotto</code> watches your git repo, sends diffs to an LLM API (e.g. OpenRouter), and surfaces a <b>cached commit message</b> in your shell when you commit.

## Install

Nothing interesting here yet but I will set something up soon. Just clone the repo and run `cargo install --path .` and then `sotto setup`

## Current state
I've only got a prototype working for the zsh side, and it is deliberately thin. Control is relenquished at the right time to allow for the cache entry to be surfaced. I just write a ([very hacky](https://github.com/cachebag/sotto/blob/master/src/shell/sotto.zsh)) widget into your zsh config.

Biggest problems now are implementing an ipc. Right now it's just the filesytem, technically. Eventually, I'd like to setup a unix socket that can get us things like realtime status changes, on demand generation and the ability to covenant over multiple repos on a machine (that is, in a better fashion than simply spawning a `sotto` process against individual repos).

Please take a look at our [issues](https://github.com/cachebag/sotto/issues) for more details.

Fish, bash, etc. is all currently in development.

## Contributing

Thank you for wanting to contribute. Please take a look at our [contributing guide](CONTRIBUTING.md) for more details.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0), or  
- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project shall be dual-licensed as above, without any additional terms or conditions.
