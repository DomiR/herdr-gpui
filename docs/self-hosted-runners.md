# Self-hosted runners

GitHub Team runs at most five hosted macOS jobs at once across the account,
and a pull request or merge queue entry asks for up to four (`checks` with
default and all features, and the two `build-macos` release builds). Runs
waited one to three hours for a macOS runner while Linux and Windows
finished in about 35 minutes. Our own Macs take those jobs instead.

## What runs where

`CI_MACOS_RUNNER` is a repository variable (Settings > Secrets and variables >
Actions > Variables) holding a JSON list of runner labels:

```json
["self-hosted", "macOS", "ARM64", "tart"]
```

When it is set, `.github/workflows/ci.yml` sends the `checks` macOS legs and
`build-macos` to runners with every label in it. `CI_LINUX_ARM_RUNNER` does
the same for the `checks` legs on `ubuntu-24.04-arm`, for example with
`["self-hosted", "Linux", "ARM64", "tart-linux"]`, on Ubuntu 24.04 VMs. The
x86_64 `ubuntu-24.04` legs stay hosted. The jobs every run waits for
(`zizmor`, `commits`, `website`, and the `checks-passed` gate) follow
`CI_LINUX_ARM_RUNNER` too: hosted runners are shared with every repository
of the account, and a run whose first job waits for one leaves our machines
idle meanwhile. Both follow one rule, members' code
only:

| Run | macOS runner |
|---|---|
| Pull request from a branch here, by the owner, a member, or a collaborator | Our Macs |
| `main`, the merge queue | Our Macs |
| Pull request from a fork | Hosted `macos-15` |
| Pull request a bot opened (Dependabot) | Hosted `macos-15` |

The author must be one GitHub lists as `OWNER`, `MEMBER`, or `COLLABORATOR`
on the pull request, so a fork's code never reaches our machines, nor does a
bot's: Dependabot's branches live here too, but bring in new dependencies'
build scripts. A member whose organization membership is private may be
listed otherwise, and then runs hosted. Windows, Nix, the
release workflow, and the iOS workflow stay hosted, as does x86_64 Linux.

A members' pull request runs its Linux checks on ARM64 rather than x86_64,
so they land on our VMs too; x86_64 Linux runs on main and in the merge
queue. Windows, hosted and the slowest platform, runs on a pull request only
for what a Windows build reads (`scripts/ci-scope.py`).

Every pull request runs the macOS default-features leg. One that needs every
platform (`scripts/ci-scope.py`) also runs the all-features leg on our Macs;
a fork's skips it, as it runs on the account's few hosted macOS runners.

Unset or empty, every job uses hosted runners, so deleting the variable is
the way back.

## Trust

A runner that outlives its job is shared by every run it takes, and a run's
build scripts and tests execute as the runner's user: they can leave a
doctored `target/`, a binary in `~/.cargo/bin`, or a LaunchAgent for the
next run. Only members' code runs on these runners, the same trust `main`
has. A runner that starts each job in a fresh Tart VM keeps nothing between
jobs, at the cost of building cold.

## Organization runners and a public repository

Runners registered on the organization join a runner group, and a group
serves public repositories only once Settings > Actions > Runner groups >
(the group) > Allow public repositories is on. This repository is public:
without it its jobs wait for a runner forever.

## Preparing a Mac

1. In the image or user the runner works as, install the Xcode command line
   tools, Homebrew, and `brew install rustup python@3.12 jq openssl@3`,
   then `rustup-init -y`. The jobs need Python 3.11 or later first on
   `PATH`, `jq`, and `brew --prefix openssl@3`.
2. Install Rosetta (`softwareupdate --install-rosetta --agree-to-license`):
   the x86_64 release build runs its CLI tests under it.
3. Register the runner with the labels `CI_MACOS_RUNNER` names, and run it
   as a service so it survives logouts.
4. Set `CI_MACOS_RUNNER` to those labels.

One runner takes one job at a time. With fewer runners than the jobs a few
runs ask for at once, runs wait for each other.

An Ubuntu runner needs passwordless `sudo` for
`scripts/install-linux-deps.sh`, Rust through `rustup` on the runner's
`PATH` (`~/actions-runner/.path`), and the packages that script installs.

## Managing the runners

`scripts/ci-runners/runners.sh` does all of this from any machine with SSH
access to the hosts, Mac minis whose CI VMs are Tart VMs kept running by
LaunchAgents named `com.sheprd.tartvm.<vm>`:

```sh
scripts/ci-runners/runners.sh resize setup warm m4-pro-1.local m4-pro-2.local
scripts/ci-runners/runners.sh wait m4-pro-1.local m4-pro-2.local
scripts/ci-runners/runners.sh status m4-pro-1.local
```

- `resize` stops each VM, gives it the sizes below (or `MACOS_CPU`,
  `MACOS_MEMORY_MB`, `MACOS_DISK_GB`, `LINUX_CPU`, `LINUX_MEMORY_MB`,
  `LINUX_DISK_GB`), and starts it again; the guest grows its file system.
  Two macOS VMs take 5 cores and 14 GB each, an Ubuntu VM 4 cores and 8 GB,
  which leaves a 12-core, 48 GB Mac mini 12 GB; giving the VMs 44 GB left
  the hosts compressing memory.
- `setup` installs what the Cirrus Labs base images lack
  (`guest-setup.sh`): Rust through `rustup`, `openssl@3` and a recent
  Python on macOS, build tools and an 8 GB swap file on Ubuntu, and the
  runner's `.path` and `.env`. The Ubuntu image has no swap, and one
  compiler process of the app takes over 5 GB: without swap the kernel kills
  compilers, and the Tart guest agent with them.
- `warm` builds `main` once in the runner's own checkout with the commands
  `ci.yml` runs on that platform (`guest-warm.sh`), so the next job compiles
  only what changed. The runner stays stopped until the build ends, and the
  build refuses to start if it cannot stop it. Keep those commands in step
  with `ci.yml`. The build runs detached inside the VM, logging to `~/sheprd-ci-warm.log` there, and skips a runner that is running a job.
- `status` prints each VM's size, whether its runner runs, and how its last
  warm build ended; `wait` waits for warm builds to end first.

Actions skip a VM whose warm build is still running, and restart a VM whose
Tart guest agent does not answer within a minute, as `tart exec` needs it. Run `setup` and `warm`
again after recreating a VM from its base image.

## The build directory

A self-hosted job checks out without `git clean` and runs
`scripts/ci-clean-self-hosted.sh`, which removes everything Git does not
track except `target/`. On a runner that outlives its job, the next build
then compiles only what changed, and the hosted Rust cache is skipped.

`target/` keeps every feature set, profile, target, and dependency version
any branch built, so the same script prunes it before a job once it passes
three quarters of `SHEPRD_CI_TARGET_LIMIT_GB`, or the disk has less than
`SHEPRD_CI_MIN_FREE_GB` free for the build about to run:

1. Incremental caches go first: they only speed up rebuilding the
   workspace's own crates.
2. Then files no build has read in `SHEPRD_CI_STALE_DAYS`, by access time:
   old dependency versions, features, and branches. Cargo rebuilds what it
   finds missing.
3. Only if it is still over the limit, or the disk still short, does all of
   `target/` go.

| Setting | Default | `setup` writes |
|---|---|---|
| `SHEPRD_CI_TARGET_LIMIT_GB` | 80 | 100 on macOS, 70 on Ubuntu |
| `SHEPRD_CI_MIN_FREE_GB` | 25 | |
| `SHEPRD_CI_STALE_DAYS` | 3 | |

Set them in the runner's `.env` (`~/actions-runner/.env`) and restart it.
In a fresh VM per job there is no `target/` to keep and the step is
harmless.
