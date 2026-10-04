# AGENTS.md

Guidance for AI coding agents (and humans) working in this repository.

## No subagents

Do not spawn subagents or delegate work to other agents (Task tools, spawned
threads, etc.). Do the work directly in the current conversation — the user
wants to follow along and a single thread of work is easier to review.

## Use Jujutsu first

This repository is a colocated Jujutsu + Git checkout. **Always use `jj` for
version-control work**, never raw Git state-changing commands:

- `jj status` / `jj diff` / `jj log` to inspect state (the working copy is
  automatically tracked as the `@` revision; there is no staging area).
- `jj describe -m "<message>"` to set the commit message on `@`.
- `jj bookmark set <name> -r <revision>` and `jj git push --bookmark <name>`
  to publish (or `jj new main` to start work on top of `main`).
- `jj git fetch` instead of `git fetch`/`git pull`.
- When the user explicitly asks to push, the agent is authorized to push with
  `jj` without requesting additional confirmation.
- Always push changes to the `main` branch. Do not create or push feature
  bookmarks unless the user explicitly asks for one.

Do not run `git add`, `git commit`, `git push`, or `git stash` — they fight
with jj's colocated state.

## Git commits

When committing, add this trailer to the commit message:

```
Assited-by: AI 
```


