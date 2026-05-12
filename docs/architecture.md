# architecture

sotto generates commit messages before you need them. this document explains why that's hard, how current solutions simply suck and how we want to solve it.

## first
i am aware of a couple things going into this project:

1. nobody cares. most people don't want to use ai to commit their changes. in fact, they are against it.
2. i am over-engineering this.

to both of those points, i say: i don't care. i think more than anything, this is a exercise in my ability to architect an elegant solution to a simple problem. 

in the end, i would say for those who _do_ want to use ai to commit their changes, this tool will bar none, be the best tool to do so.

## motivation

while i think [the leading](https://github.com/nutlope/aicommits) project in this space works _okay_, i still do not enjoy the friction around getting a commit into my repo. there is no reason i should be placed in a new prompt or session to commit my changes. it's too much ceremony for something that should be mindless.

this led me to think about what the best way to approach this problem is. because as interesting as `aicommits` is, it simply _still_ does not solve the problem at hand: finding the right commit message for your changes as quickly as i did before ai existed. while i am no fan of ai, if it can save me whatever amount of time i spend over the course of years, thinking of and writing a commit message, that would be a win.

so over the weekend, i came up with `sotto`. no one actually cares about the generation of the commit message and you don't actually need any visual indicator at all that a commit message is being generated. it should just be there. and if you don't like the message, simply don't use it. 

the work is instead offloaded to _how_ we present this to the user. when i stage my changes and begin writing `git commit -m ...` i do not want to have to now wait for an llm to read my diff and think of a message. i just want the message to be there. so let's just do all the busy work in the back. let's keep the llm on standy until a user signals that they are about to commit and push some work. 

i intentionally want `sotto` to live completely unbeknownst to me or my machine. it should only exist when we need it to, i.e., when we stage/commit our changes. so that's what it does and what it will do better, as i continue working over the concept better.

## the problem

every ai commit tool works the same way: you run `git commit`, it reads your diff, calls an api, and you wait. the wait is usually 1-3 seconds. in some cases, [far longer](http://cachebag.sh/journal/sotto/) that's an eternity when you're zoned in working on something.

it turns out that the problem isn't the llm: it's the timing of everything. generation happens at exactly the wrong moment, when you're ready to commit and want to move on.

## the insight

commit messages can be generated speculatively. we know that the diff exists the moment you save a file. if we generate then, the message is ready before you need it. 

this only works if:

1. we know what diff to use (staged vs unstaged, which matters)
2. we don't waste api calls on noise (rapid saves, unchanged content)
3. failures are invisible (sotto broken = git works normally (**this is very crucial**))

## the architecture

```
[file watcher] → [debounce] → [diff] → [hash] → [generate] → [cache]
                                                                 ↓
                                                 [shell] ← [read cache]
                                                    ↓
                                              [ghost text]
```

as you can see, we can solve this with two processes, and one file. the daemon writes, the shell reads, and there is no coordination needed between the two.


### daemon

if you aren't familiar, a daemon is a process that runs in the background and is not tied to a terminal. it is a long-lived process that can be started and stopped on demand.

in our case, it is a background process that watches your repo and keeps a commit message "warm".

**watching:** recursive file watcher on the working tree. filters out `.git/` internals and anything in `.gitignore`. reacts to real changes only.

**debouncing:** rapid saves don't trigger rapid api calls. we wait for a pause in activity before generating. but `.git/index` changes (staging) get priority. we treat that as the sign that you're about to commit.

**diffing:** staged changes take precedence. if you've run `git add`, we diff exactly what will be committed. if nothing is staged, we speculatively diff the working tree. staged always wins because it's the truth.

**hashing:** every diff is hashed. if the hash matches the last generation, we skip. this prevents redundant calls when you save without meaningful changes or stage/unstage the same files.

**generating:** minimal prompt, low max tokens. if you really wanted, you can have your own prompt that matches your conventions in commiting. but it's far more important to understand that the llm call is the least interesting part of the system.

**caching:** write the message to disk, keyed by repo path. the shell reads this file. if the daemon crashes, the last message is still there.

### ipc

the daemon broadcasts state transitions over a unix socket: debouncing, generating, ready, error. this enables:

- live status indicators (spinner while generating)
- on-demand regeneration (user rejects suggestion, requests new one)  
- precision timing (shell signals "user is at git commit", daemon prioritizes)

the socket is push-based and non-blocking. each client gets a dedicated writer thread. dead clients are pruned instantly. the daemon never blocks on i/o.

### shell

a thin layer that reads the cache and renders ghost text when you type `git commit -m '`. 

the shell does almost nothing by design:

- read a file
- print it as ghost text  
- accept on tab

no network calls. no diffing. no generation. if `sotto complete` takes more than a few milliseconds, something is wrong.
> [!NOTE] we should benchmark this somehow...

sotto must be invisible to normal git operations. if anything fails, such as no daemon, no cache, no config, then the shell prints nothing and exits cleanly. git works exactly as it always has. we do not want to mess with any of that.

## design principles

**generate before the user needs it.** the entire architecture exists to solve a _timing_ problem. we shift work from commit-time to save-time. we should not have to sit and watch a spinner when we're ready to commit.

**failures are invisible.** sotto is additive. if it breaks, git doesn't. no error messages, no blocked commands, no degraded experience. i keep stressing this point because it is important.

**the daemon is the product.** watching, debouncing, diffing, hashing. we treat the shell as a rendering layer and the llm is a just a commodity.

**staged diff is truth.** working tree diffs are speculative. staged diffs are what will actually be committed. when both exist, staged should alwasy take precedence.

## future directions (WIP)

none of these thoughts are fleshed out..

**multi-repo daemon:** single long-lived process managing watchers for multiple repos. the shell tells it which repo to watch on `cd`. idle repos time out.

**context layer:** per-file history of changes, summaries, invariants — accumulated from sotto's commit data over time. a codebase knowledge graph that agents and reviewers can query. this is a separate tool that consumes sotto's output; sotto stays thin.
