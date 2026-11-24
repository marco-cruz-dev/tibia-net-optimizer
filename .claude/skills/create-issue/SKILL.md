---
name: create-issue
description: Create a GitHub issue in marco-cruz-dev/tibia-net-optimizer with this repo's labels, milestone and body template, and add it to the "Tibia Network Optimizer App" project. Use when asked to file, open, track or plan a bug, feature, chore, spike or follow-up.
---

# Create an issue

Issues and the project board (https://github.com/users/marco-cruz-dev/projects/2)
are the backlog. There is no backlog file.

## 1. Draft and confirm

Check for duplicates first:
`gh issue list -R marco-cruz-dev/tibia-net-optimizer --state all --search "<keywords>"`.

Show the user the title, labels, milestone and body, and wait for approval.

- **Title:** short imperative sentence, no ID, no trailing period.
  "Keep quickack on without NetworkManager".
- **Labels:** exactly one of each:
  - `type: feature | bug | chore | docs | refactor | test | spike`
  - `area: windows | unix | core | gui | monitor | infra`
  - `priority: p0` (blocks the milestone) `| p1` (should be in it) `| p2` (nice to have)
- **Milestone:** the next unreleased version (`gh api repos/marco-cruz-dev/tibia-net-optimizer/milestones -q '.[].title'`).

Body:

```markdown
## Summary
What changes, in one or two sentences.

## Why
The player-facing problem or the risk it removes.

## Acceptance criteria
- [ ] Concrete, testable outcomes
- [ ] Failing test first; `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass
- [ ] Real OS adapters have an `#[ignore]`d read-only smoke test
- [ ] README updated; untestable steps added to the manual QA checklist (#55)

## Out of scope
Optional: what this issue deliberately doesn't cover.
```

## 2. Create and add to the project

```sh
url=$(gh issue create -R marco-cruz-dev/tibia-net-optimizer \
  --title "<title>" --body-file <file> --milestone "<milestone>" \
  --label "type: …" --label "area: …" --label "priority: …")
item=$(gh project item-add 2 --owner marco-cruz-dev --url "$url" --format json -q .id)
```

Then set **Status = Backlog** (look the IDs up, don't hard-code them):

```sh
pid=$(gh project view 2 --owner marco-cruz-dev --format json -q .id)
gh project field-list 2 --owner marco-cruz-dev --format json \
  -q '.fields[] | select(.name=="Status") | .id, (.options[] | select(.name=="Backlog") | .id)'
gh project item-edit --project-id "$pid" --id "$item" --field-id <status id> --single-select-option-id <backlog id>
```

Reply with the issue URL.
