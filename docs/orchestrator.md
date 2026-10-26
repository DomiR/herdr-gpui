# Orchestrator

The orchestrator (`crates/herdr-gpui/src/orchestrator/`) lists a repository's
issues, pull requests, and the agent runs dispatched for them in the activity
bar's Pull requests and Issues sections, and dispatches new ones through the
Herdr CLI. User-facing behavior is in the crate README's "Orchestrator"
section; this page is for working on it.

## State and restart

The orchestrator keeps its own per-repository SQLite database, in the app's
state directory: `<state dir>/repositories/<uuid v5 of the canonical git
common dir>/state.sqlite3`, private to the user. Repositories on SSH hosts
are keyed by `ssh://<destination><git common dir>` and live on this machine.
Its schema began as agent-launcher's (`PRAGMA user_version`, currently 2,
which added each open pull request's last read review and checks):

- The first time a repository opens, agent-launcher's database at
  `<data_local_dir>/agent-launcher/repositories/<same uuid>/state.sqlite3`
  is copied in, if there is one, so its items and runs come along. Nothing
  is written back to it afterwards.
- Runs keep their `runs.owner` (`agent-launcher` or `herdr-gpui`); only
  herdr-gpui's own runs are stopped, messaged, or removed.
- A source's issues are replaced only with a complete listing. A capped
  listing, or one made while the schema is newer than this build knows, is
  shown and not written. A newer schema opens read-only, with a banner.

A restart shows what the last run found before any network or git call:
the repository each checkout held comes from the app's cache database
(`<state dir>/cache/cache.sqlite3`, `crate::cache`), then the items, runs,
and checks from the repository's database. Detection and the first sync
follow at once and replace it, and syncing then runs on its timer. A
checkout that no longer holds that repository drops what was shown.

`store.rs` implements the schema; its tests open a version-0 database as
agent-launcher wrote it and check that migration keeps its rows.

## Layout

| Module | Responsibility |
| --- | --- |
| `model.rs`, `location.rs`, `store.rs` | Records in the database's shapes, its path, and SQLite access. |
| `repo.rs` | Finding a checkout's repository, main checkout, Beads, and remote on its host. |
| `github.rs`, `gitlab.rs`, `beads.rs` | The sources: a full listing each. GitHub reads only what changed since the last sync, with a full listing again every 30 minutes; GitLab lists its open issues in full on every sync. |
| `linear.rs` | The Linear source, in `herdr-orchestrator`: its API key (`linear/key.rs`, saved through `herdr-credentials`), GraphQL transport (`linear/http.rs`), listing, and new issues (`linear/create.rs`). The app opens Linear links in the Linear app (`orchestrator/linear_link.rs`). |
| `review.rs` | Each open pull request's review decision, review requests, and checks, read after every GitHub sync and kept in the database, so a restart shows the last ones until the first sync. |
| `service.rs` | The worker thread per open view: sync, reload, and the action queue. |
| `actions.rs` | Dispatch, stop, send, remove, delete: each on its own thread. |
| `prompt.rs` | Prompt profiles, agent-launcher's built-in and review prompts. |
| `naming.rs` | agent-launcher's branch names, and runs found from branches. |
| `view/` | The view entity: the list of every repository (`view/repo.rs` keeps each one's worker, snapshot, and rows; an item's page and actions go to its own), the backend and project pills, filters, the item pane, dialogs. Pure row, merge, and grouping logic is in `view/rows.rs`. |
| `host.rs` | The window's one view and the repositories it lists, one per Git common directory per host across every connected host's workspaces: the tick that feeds it, and its events. |
| `section.rs` | The Pull requests and Issues sections, and the commands that open them narrowed to a project. |

The view is a GPUI entity that talks to its host only through `Event`s and
setters (`set_look`, `set_live`, `set_github`, `set_hosts`). Tabs hosted it
before the activity bar; tabs saved by those builds are dropped on load. Nothing in it blocks: the worker does every disk, network, and host
script call, and the UI reads the newest `Snapshot` from a coalescing mailbox.

Live agent status comes from each connected host's Herdr snapshot, matched to
runs by host, workspace, and pane; it is never written back to the database.

Dispatch can receive `agent_pane_busy` while the new worktree's shell runs its
startup hooks. It retries only that structured rejection for up to ten seconds,
checking the terminal identity returned by worktree creation before each retry.
An accepted launch, transport failure, or agent-readiness timeout is never
replayed. Waiting happens on the action worker and observes cancellation;
permanent failures retain the normal failed-run recording and worktree cleanup.

## Branches name their item

Dispatch names a branch as agent-launcher does: `agent/<identifier>-<title>-<hash>`,
where `<hash>` is the first eight hex digits of a UUIDv5 (URL namespace) of the
item's canonical key. A read-only review uses `agent/review-<identifier>-<title>-<hash>`.
The hash makes the branch the item's own, so work is found without a database:

- a Herdr workspace on a connected host whose branch ends in an item's hash,
  working when an agent is in it and stopped when none is;
- a branch under `refs/heads/agent/` in the repository, read on each sync, that
  no workspace shows.

Each becomes a run owned by `branch`, shown like any other but never written,
and only when no recorded run already has that branch. It opens its workspace;
it has no Stop, Send, or Remove, since no application owns it.

## Linear

Linear lists open issues across one workspace, keyed
`linear:linear.app:<workspace urlKey>`. The worker's Linear parts are in
`service/linear.rs`; the Connect Linear dialog is `view/linear.rs`.

- A sync runs one query per `linear::Scope`: `Mine` (assigned to whoever owns
  the key) and `Cycles` (the active cycle of any team that person belongs to),
  both open issues only, at most 500 each, deduped by identifier. One query
  with an `or` filter would be cheaper, but Linear answered it with rows
  matching neither side. A sync costs about five requests against Linear's
  limit of 2,500 an hour.
- What Linear says that the shared schema cannot hold (the branch name, the
  assignee and whether it is the key's owner, the cycle) is the item's
  `Tracker`, never stored; rows read from the database have none. The Issues
  filters column's Mine and Current cycle read it, so they never pass a GitHub
  issue or a bead. `rows::linear_defaults` turns both on the first time
  tracked items arrive, once per view and never after a filter click.
- A view runs one worker per project, but Linear belongs to the user, so only
  one worker's `Request::linear` is set and Linear is listed once.
  `OrchestratorView::set_repos` keeps it on a kept worker that already lists
  Linear, else gives it to the first project, restarting that worker when the
  project that listed Linear is gone. Dispatching on a Linear issue therefore
  starts the agent in that project's repository.
- Linear items list only in the activity bar's Linear section. The one
  orchestrator view the sections share has an `IssueSource`: Issues sets
  `Repositories`, Linear `Linear`, and `listing.rs` keeps each source's
  providers. Each source keeps its own `Filters`, swapped as the source
  changes, and `default_filters` turns Mine and Current cycle on in Linear's
  only, since they hide every issue without a tracker. The header's source
  chips follow the source, so Linear connects from its own section.
- Moving a card between lanes sends `issueUpdate(id, input: { stateId })`
  through the worker that lists Linear, so the key stays there. Each sync
  also reads every team's workflow states (their ids), and each issue its
  UUID and team, kept on its `Tracker`; a failed states query only leaves
  the cards unmovable. The view shows the move at once and keeps it until
  the worker's snapshot reports that move's sequence number done; a failure
  puts the card back with a notice, and a sync published before the move
  finished never undoes it (`view/moves.rs`).
- Each issue's `sortOrder` (Linear's manual order) is kept as
  `Tracker::sort_order`; board lanes sort by it, and a drop between two
  cards sends the midpoint of their orders (one past an end), with the
  state when the lane changes, in the same `issueUpdate`. The board's scroll
  handles (lane row and each lane, by status name) live on the view
  (`board::Scrolls`), since element state is dropped while the section is
  not drawn. Hidden statuses (`view/hidden.rs`) are a set of status names
  that drops those issues from the list and those lanes from the board.
- New issue (the header button, or C while the list has focus) opens a
  dialog like Linear's (`view/create.rs`). The worker that lists Linear reads
  the pickers' choices with one query the first time it opens
  (`linear/create.rs`), keeps them until a read fails or the key changes,
  and sends `issueCreate` with only the fields that are set
  (`service/linear_create.rs`), so the key never leaves the worker. A new
  issue starts in the team's default state (else its first unstarted or
  backlog state), assigned to the key's owner, in the active cycle; a
  created issue syncs the list, and its page opens once the list shows it.
- The List layout reads as Linear's own list (`view/linear_groups.rs`,
  `view/linear_rows.rs`): by assignee, you first and unassigned last, then
  by state in board order, folding through the view's `collapsed` set
  (`group:` keys). Board cards are drawn as Linear's (`view/linear_card.rs`),
  sharing their parts with the rows (`view/linear_parts.rs`); a lane's head
  goes through `kanban::Lane::render_with`. The query also reads `estimate`,
  the assignee's `avatarUrl`, `parent`, `project`, GitHub pull request
  `attachments`, and the first ten `children` states for sub-issue progress
  (marked `+` past ten), all kept on the `Tracker`, never stored.
- **Go to worktree** opens an issue's newest run the way a run's Open does
  (`view/worktree.rs`), and remembers the issue: while the window is on
  Workspaces with it pending, Toggle Sidebar (Cmd-B) and Back return to the
  Linear section with its page open, once.
- A Linear issue's `Tracker::status` is its workflow state (`state { name
  type position color }`), never stored. The view's `status` narrows to one
  state, like `label`; `view/board.rs` lays the listed rows out in a lane
  per state, ordered by `StatusKind` then position, with the shared lanes
  and cards in `crate::kanban` that Tickets (native) also draws.
- Linear items are never written to the shared `issues` table or
  `source_checkpoints`. agent-launcher's `load_issues` reads every row and its
  `issue_from_row` fails on a provider other than GitHub, GitLab, or Beads, so
  one `linear` row would break its inbox for the repository; it also prunes
  sources it does not sync. Every listing stays in `Worker::uncached`. Runs on
  Linear issues are written, since agent-launcher treats `runs.issue_key` as
  opaque text.
- `native_id` is the issue's identifier (`ENG-12`), not its UUID, so a run
  whose issue left the listing (done, or unassigned) still shows a readable
  stand-in. Moving an issue to another team changes its identifier and orphans
  such runs.
- The listing asks Linear to sign the file links in its answer
  (`public-file-urls-expire-in`, an hour), so images in a description load
  from `uploads.linear.app` without the key, which never leaves the worker.
  `crate::markdown_view` caches them by address without the signature.
- The key is read when the view opens, on Refresh, and after a save, never on
  the periodic sync: a locked macOS Keychain would ask every minute in every
  open view. `LINEAR_API_KEY` wins over the saved key.
- Dispatch uses the issue's `branchName` when git accepts it. Such a branch has
  no hash, so a branch also names a Linear issue when it holds the issue's
  identifier as a token: a letter, letters or digits, a dash, and all the digits
  after it, not preceded by a letter or digit. `dominique/eng-12-fix` names
  `ENG-12`; `eng-123-x` and `xeng-12` do not. A hash match wins. Besides the
  `refs/heads/agent/` read, each sync reads the 500 most recently committed
  branches and keeps those that could name an item.

## GitLab

`gitlab.rs` lists a GitLab project's open issues through the REST API
(`/api/v4/projects/<escaped path>/issues`), on gitlab.com or a self-managed
host, in agent-launcher's GitLab row format: `native_id` is the issue's
project `iid`. The API is reached over HTTPS, or over HTTP when the remote's
own URL is `http://`; the shared source key names only the host, so the scheme
is read from the remote each time.

A remote whose host the GitLab CLI names as an instance's `ssh_host` is read
from that instance instead: requests go to its `api_host` (the instance's own
name when unset) over its `api_protocol` (HTTPS when unset), and its token is
the instance's. This is how a self-managed GitLab with a separate SSH host,
such as `gitlab-ssh.example.com` beside `gitlab.example.com`, is reached; the
SSH host serves no API. The mapping comes from the `hosts` table of glab's
`config.yml`, found where glab looks (`GLAB_CONFIG_DIR`, the legacy
`~/.config/glab-cli`, else `glab-cli` in the platform's config folder or
`XDG_CONFIG_HOME`), and is read again on every sync. Only the block form glab
writes is read; glab has no command that lists its hosts. A remote on an
instance's own host, or on a host glab does not know, keeps its own host and
the remote's scheme, as before, and the source key always keeps the remote's
host, as agent-launcher keys it. A failed connection names the address it
tried.

A token is sent only over HTTPS: over HTTP
the read is anonymous, and a project that is not public asks for an
`https://` remote. Pages follow a cursor on creation time
(`order_by=created_at`, `created_after`), which edits and closes cannot shift. Each sync is a complete listing of at most 2,000 issues, so it
may replace the cache; a larger project is shown and not written, as GitHub's
is.

Its token is looked up once per view on the worker, and again on Refresh or
when the remote's instance changes:

1. `GITLAB_TOKEN`, then `GITLAB_ACCESS_TOKEN`, only for the instance
   `GITLAB_HOST` names (gitlab.com when unset), so a gitlab.com token never
   reaches another host;
2. else `glab config get token --host <instance>`, the GitLab CLI's own sign-in,
   with its environment tokens removed and the shared bounded process policy;
3. else none: public projects still list, and a private one asks for a token.

Merge requests are not listed yet. They would need `PullRequest`-like
metadata in the shared schema, which is a contract change for both
applications, and herdr-gpui has no native GitLab sign-in.
