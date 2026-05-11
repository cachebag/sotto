# architecture

this is a deep dive into sotto's internals. it covers the daemon, the cache, the shell integration, and the design decisions behind each. if you're contributing or just curious about how it works under the hood, this is the doc.

## high level

sotto has three moving parts:

1. **daemon** — a background process that watches your repo, diffs on change, generates a commit message via an llm, and writes it to a flat file cache.
2. **cache** — the bridge between the daemon and the shell. just files on disk keyed by repo path hash.
3. **shell completer** — a zsh/fish widget that reads the cache and surfaces the message as ghost text when the user types `git commit -m '`.

the daemon does all the heavy lifting. the shell does almost nothing. this is intentional — the shell layer should be as thin and dumb as possible. all it does is read a file and render it. the daemon is where the intelligence lives.

```
[file watcher] → [debounce] → [git2 diff] → [hash compare] → [llm api] → [cache write]
                                                                                ↓
                                                              [shell widget] ← [cache read]
```

## daemon

lives in `src/daemon/`. three files:

### watcher.rs

the core loop. this is where sotto spends most of its time.

- opens the repo via `git2::Repository::discover(".")`, which walks up from cwd to find the repo root. this means the user can be in any subdirectory.
- sets up a `notify` file watcher on the working tree. watches recursively.
- filters out noise: anything inside `.git/`, anything matched by `.gitignore` (via `repo.status_should_ignore()`). this means `node_modules`, `target/`, build artifacts — all handled by whatever the repo already ignores.
- on file change, resets a debounce timer. the timer duration comes from `config.debounce_secs` (default 5s).
- when the timer fires, runs a diff.

the diff strategy depends on what's available:

- if there are staged changes (`git diff --cached` equivalent via `repo.diff_tree_to_index()`), use those. this is the precise signal — it's exactly what will be committed.
- if nothing is staged, fall back to the working tree diff (`repo.diff_index_to_workdir()`). this is speculative — it's what the user has changed but hasn't staged yet.

staged always wins. if the user has both staged and unstaged changes, sotto generates the message based on what's staged because that's what `git commit` will actually capture.

the diff is accumulated into a string via `diff.print(DiffFormat::Patch, ...)` and capped at `config.max_diff_lines` to prevent blowing up token budgets on massive diffs. if truncated, a marker is appended so the llm knows the diff is incomplete.

after getting the diff string, it's hashed (sha256). if the hash matches the last hash we generated for, we skip — nothing meaningfully changed. this prevents redundant api calls when the user saves without making real changes, or stages and unstages the same files.

if the hash is new, the diff is handed off to the generator, and the result is written to cache.

### debounce behavior

the debounce exists to avoid burning api calls on rapid saves. but it's not one-size-fits-all:

- for small diffs (under ~20 lines), the debounce can be skipped entirely. a one-line change is cheap to generate and the user is more likely to commit quickly after a small edit.
- for `.git/index` changes (meaning `git add` just happened), the debounce should be very short (1-2s). this is the strongest signal that a commit is imminent. by the time the user finishes typing `git commit -m '`, the generation should already be done.
- for working tree changes (regular saves), the full debounce applies. the user is mid-session, no rush.

this two-tier debounce — aggressive on index changes, lazy on working tree changes — is how sotto stays fast without being wasteful.

### generator.rs

takes a diff string, builds a prompt, posts to the configured endpoint, returns the commit message.

the prompt is minimal:

- system message: "you are a concise git commit message generator. given a diff, write a single-line commit message. use conventional commit format. no explanation, no quotes, just the message."
- user message: the diff itself.

`max_tokens` is set low (50-100) because commit messages should be short. anything longer than one line is a bad commit message regardless of who wrote it.

the http call uses `ureq` (synchronous, no async runtime needed). the daemon is single-threaded and blocking by design — it only needs to make one api call at a time, and it's doing it in the background where latency doesn't matter.

the endpoint and model are configured in `config.toml`. sotto defaults to openrouter's free tier but any openai-compatible api works. swap the endpoint and model string, done. no provider-specific code.

### cache.rs

two functions: `read` and `write`. that's it.

the cache layout on disk:

```
~/.local/share/sotto/cache/
  <sha256 of repo workdir path>/
    message       # plain text, the generated commit message
    diff_hash     # plain text, the sha256 of the diff it was generated from
```

`write` creates the repo directory if it doesn't exist, writes both files. `read` returns `Option<CacheEntry>` — a cache miss is not an error, it's just "nothing cached yet."

the cache is the ipc layer for v1. the daemon writes, the shell reads. no sockets, no shared memory, no coordination. just the filesystem. this is intentionally simple — it works, it's debuggable (`cat` the file), and it's resilient (if the daemon crashes, the last cached message is still there).

the diff_hash is stored alongside the message so that future versions can do staleness checks — "this message was generated from diff X, but the current diff is Y, so it might be stale." not used yet, but the data is there.

## shell integration

lives in `src/shell/`. two concerns: the completion binary and the shell widgets.

### complete.rs

the `sotto complete` subcommand. called by the shell widget, not by the user directly.

- discovers the repo from cwd
- hashes the repo path
- reads the cache entry for that repo
- prints the message to stdout
- exits

this function never errors. if anything fails — no repo, no cache, no file — it prints nothing and exits 0. the shell widget interprets empty stdout as "no suggestion." sotto should never interfere with normal git operations. if sotto is broken, git should still work exactly as it always has.

### zsh widget (sotto.zsh)

the zsh integration hooks into zle (the zsh line editor) to render ghost text via `POSTDISPLAY`.

key components:

- `_sotto_ghost` — a `line-pre-redraw` hook. fires on every keystroke. checks if the buffer matches `git commit -m '...` or `git commit -m "...`. if not, does absolutely nothing — no state changes, no highlight manipulation, completely inert. if it does match, calls `sotto complete`, sets `POSTDISPLAY` to the result, and applies `fg=8` (grey) styling via `region_highlight`.
- `_sotto_accept` — bound to Tab. if sotto is actively showing a suggestion (`_sotto_active` flag), it rebuilds the buffer with the suggestion inserted between quotes. if sotto is not active, passes through to `zle expand-or-complete` (default tab behavior).
- `_sotto_active` — flag that tracks whether sotto is currently rendering a suggestion. prevents sotto from intercepting tab when it shouldn't.
- `_sotto_accepted` — flag that prevents the ghost text from re-appearing after the user has already accepted a suggestion. reset when the buffer changes to a non-commit command.

important constraints:

- sotto must not interfere with `zsh-autosuggestions`. it calls `autosuggest-clear` if the widget exists (`${+widgets[autosuggest-clear]}`), otherwise does nothing.
- sotto must not clobber `region_highlight`. it only appends its own entries and only removes entries it created. other plugins (syntax highlighting, etc.) are left untouched.
- sotto must not affect tab completion outside of `git commit -m`. the `_sotto_active` check ensures tab passes through to the default handler for every other command.

### fish widget (sotto.fish)

similar approach but using fish's keybinding system. fish doesn't have `POSTDISPLAY` so the implementation differs — it hooks into the keybinding layer and uses `commandline` builtins to manipulate the input.

### shell injection

during `sotto setup`, the widget script is written to `~/.local/share/sotto/shell/` and a source line is appended to the user's rc file (`.zshrc` for zsh, `conf.d/` for fish). `append_if_missing` ensures running setup twice doesn't duplicate the source line.

the source line must come after plugin managers (zinit, oh-my-zsh, etc.) in the rc file so that sotto's keybindings aren't overwritten by plugins that load later.

## config

lives in `src/config.rs`. two structs:

### Paths

resolves platform-aware directories using the `dirs` crate:

```
~/.local/share/sotto/     # state: cache, socket, logs
~/.config/sotto/          # config: config.toml
```

`init_dirs()` ensures all directories exist. called once at startup before anything touches the filesystem.

### SottoConfig

deserialized from `config.toml` via `serde` + `toml`. fields:

- `endpoint` — the llm api url. default: openrouter.
- `model` — the model string. default: `openrouter/free`.
- `api_key` — the api key.
- `debounce_secs` — how long to wait after the last file change before generating. default: 5.
- `max_diff_lines` — truncate diffs beyond this many lines. default: 500.

two load methods:

- `load()` — returns `Result`, used by `sotto setup` and `sotto daemon` where errors should surface.
- `load_silent()` — returns `Option`, used by `sotto complete` where errors should be swallowed silently.

## cli

`main.rs` wires three subcommands via `clap`:

- `sotto setup` — runs the setup wizard (provider selection, api key, shell detection and injection).
- `sotto daemon` — starts the file watcher. currently foreground, will be backgrounded.
- `sotto complete` — prints cached message to stdout. called by shell widgets, not by users.

`sotto complete` swallows all errors. `sotto setup` and `sotto daemon` surface errors with context via `anyhow`.

## design principles

**sotto failing should be invisible.** if the daemon crashes, if the api is down, if the config is missing, if the cache is empty — the user should never see an error. git works exactly as it always has. sotto is additive, never blocking.

**generate before the user needs it.** the entire architecture exists to solve a timing problem. every other tool in this space generates at commit time and makes the user wait. sotto generates while the user is working so the message is already cached when they need it.

**the shell layer is dumb.** it reads a file and renders it. no network calls, no diffing, no generation. if `sotto complete` takes more than a few milliseconds, something is wrong.

**the daemon is the product.** the watcher, the debounce strategy, the hash comparison, the two-tier diff (staged vs working tree) — this is where the complexity lives and where the value comes from. the llm call itself is the least interesting part.

## future architecture (not in v1)

### unix socket ipc

replace the flat file cache with a unix socket for real-time communication between daemon and shell. enables:

- status queries (is the daemon running? is a generation in progress?)
- on-demand regeneration (user rejects suggestion, requests a new one)
- precision pass (shell tells daemon "user is at git commit, run staged diff now")
- multi-repo coordination (shell tells daemon which repo to watch on cd)

### per-file context layer

each file gets a uuid and a running history of its changes — summaries, notable behaviors, invariants. accumulated from sotto's commit data over time, editable by the developer for context an llm can't infer from diffs alone.

this is not a sotto feature. it's a separate tool that consumes sotto's output. the commit messages and diffs sotto already generates become the data stream for a codebase knowledge graph that agents and reviewers can query. sotto stays thin. the context layer builds on top.

### multi-repo daemon

single long-lived daemon process that manages watchers for multiple repos. the shell sends "watch this path" messages over the unix socket on `cd`. repos are tracked with idle timeouts — stop watching after N minutes of inactivity. the swarm idea: on install, walk the user's configured workspace paths, discover all repos, pre-seed the cache. first-run experience is instant.