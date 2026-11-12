# POS Git workflow (lightweight GitFlow)

This project uses two **permanent** branches, plus temporary working branches.
The goal is to keep the stable release line separate from work in progress.
No new POS version is released or tagged merely by creating these branches.

| Branch | Purpose | Creation / merge |
| --- | --- | --- |
| `main` | Stable, releasable branch; source for release tags | Only via `release/*` or `hotfix/*` PR |
| `develop` | Integration branch for the next release | Via feature/fix/docs PRs; receives release/hotfix fixes |
| `feature/<topic>` | New functionality | From `develop`, PR to `develop` |
| `fix/<topic>` | Nonurgent fixes | From `develop`, PR to `develop` |
| `docs/<topic>` | Documentation updates | From `develop`, PR to `develop` |
| `release/<X.Y.Z>` | Release stabilization | From `develop`, PR to `main`, then integrate into `develop` |
| `hotfix/<X.Y.Z>` | Critical production fix | From `main`, PR to `main`, then integrate into `develop` |

## Everyday development

Start from a clean repository, with your changes committed or stashed:

```sh
git fetch origin
git switch develop
git pull --ff-only origin develop
git switch -c feature/task-priorities
# Implement, test and commit.
git push -u origin feature/task-priorities
gh pr create --base develop --head feature/task-priorities
```

Use `fix/*` for ordinary bugs and `docs/*` for documentation changes.
The PR must pass the `verify` GitHub Actions job. Nothing merges automatically.
An agent must still obtain explicit user authorization for pushes/merges,
as described in the repository `AGENTS.md`.

## Prepare a release

Create a temporary release branch from tested `develop`:

```sh
git fetch origin
git switch develop
git pull --ff-only origin develop
git switch -c release/0.1.0
# Only stabilization, documentation, and release fixes from here.
git push -u origin release/0.1.0
gh pr create --base main --head release/0.1.0
```

After approval and successful CI, merge the release PR into `main`, then tag
the **actual merged commit on main** as `v0.1.0`. Push the tag only when the
release is explicitly authorized. Merge any release fixes back into `develop`
using a separate PR before deleting the release branch. The repository
currently does not enable automatic branch deletion after merge.

An exact release number, version bump and changelog are decided at release
time. Do not invent or automatically publish release tags.

## Emergency hotfix

Start from `main` so the fix includes only released code:

```sh
git fetch origin
git switch main
git pull --ff-only origin main
git switch -c hotfix/0.1.1
# Implement the minimal production fix, then test and commit.
git push -u origin hotfix/0.1.1
gh pr create --base main --head hotfix/0.1.1
```

After approval and CI, merge into `main`, tag the new patch release, and bring
the hotfix changes into `develop` through a PR. Release and hotfix branches
are temporary, not long-lived deployment environments.

## Protections and conventions

- Both `main` and `develop` are protected. PRs and passing `verify` checks are
  required; force pushes and branch deletion are disabled.
- CI rejects PRs into `main` unless their source branch starts with
  `release/` or `hotfix/`.
- Never commit personal SQLite databases, secrets, `.env` files or build
  outputs. PRs are reviewed for data migrations and destructive changes.
- Keep `main` as the GitHub default branch to point new clones at stable code.
  This means the **PR base must be set explicitly to `develop`** for routine
  changes. GitHub defaults PRs to `main` unless the base is changed.
- Squash merges are suitable for most small features. Preserve release
  integration as needed; use release tags to identify actual shipped commits.
- Rules apply to the GitHub repository. Local feature branches and the
  protected permanent branches remain ordinary Git refs.
