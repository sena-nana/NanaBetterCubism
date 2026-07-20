---
name: lilia-app-git
description: Git workflow for final Lilia desktop app changes. Use for staging, committing, pushing, merging, syncing dependencies, reviewing diffs, or preserving user and other-agent changes.
---

# Lilia App Git

## MUST

- MUST inspect `git status --short` before staging. Use `git diff`, `git diff --numstat`, and `git diff --check` when the change is non-trivial, cross-module, generated, or near user edits.
  - Reason: 盲暂存会混入无关产物或覆盖他人改动。
- MUST stage only files that belong to the current task. MUST NOT stage unrelated generated output, caches, build artifacts, local secrets, or user work.
- MUST write the commit title as a short Chinese sentence summarizing the result. Add a body only when it clarifies concrete changes; keep it a short list.
  - Reason: 中文短句符合本仓库提交风格，便于检索。
- MUST run `git fetch origin` before trusting ahead/behind counts for sync or merge work.
  - Reason: 本地 ref 过期会误判同步状态。
- MUST push only after the intended commit or already-ready branch state is confirmed.
- MUST keep dependency update commits dependency-only unless the user asked for extra work.

## MUST NOT

- MUST NOT revert or overwrite user or other-agent changes unless the user explicitly requests it.
  - Reason: 覆盖他人改动破坏协作且可能丢失工作。

## SHOULD

- Before committing logic, refactor, public module, or shared behavior changes, do a quick self-check for duplicate branches, redundant helpers, dead state, and comments that only restate behavior.
- When pulling or merging with local changes, check whether upstream changes touch the same files before proceeding.
- After changing LiliaUI dependencies or lockfiles, include the validation required by `$nanabettercubism-validation`.

## See also

- `$nanabettercubism-validation` for post-change validation.
