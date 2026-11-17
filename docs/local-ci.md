# Running CI locally with `act`

[`act`](https://github.com/nektos/act) runs the jobs of
`.github/workflows/ci.yml` in Docker containers on your machine, with the same
steps as GitHub's runners. Use it to reproduce a CI failure, or to run the
whole quality gate before a release, without spending runner minutes.

The workflows are not changed for `act`: `.actrc` at the repository root holds
the local defaults, and `act` reads it from the directory you run it in.

## Setup

- Docker, with your user in the `docker` group.
- `act` 0.2.94 or later. Download the release archive for your platform from
  the project's releases page, check it against `checksums.txt`, and put the
  binary on your `PATH`.
- About 15 GB of free disk for the images, the containers' builds and the
  local cache.

`.actrc` maps the GitHub runner labels to the `catthehacker/ubuntu` images:

| Runner label | Image |
| --- | --- |
| `ubuntu-latest`, `ubuntu-24.04` | `catthehacker/ubuntu:act-24.04` |
| `ubuntu-22.04` | `catthehacker/ubuntu:act-22.04` |

It also keeps the `test` matrix to `os: ubuntu-latest` (Docker cannot run the
Windows and macOS runners) and runs two jobs at a time, so that two parallel
Rust builds fit in memory. `act` refuses flags that reach the host's files in
a project `.actrc`; pass those on the command line.

## Commands

From the repository root:

```sh
act -l                                         # list the jobs
act push -W .github/workflows/ci.yml           # the whole CI workflow
act push -W .github/workflows/ci.yml -j clippy # one job
act push -W .github/workflows/ci.yml -j test   # tests and golden traces (Linux)
act push -W .github/workflows/ci.yml -j msrv   # the 1.85 MSRV run
```

After the first run, add `--pull=false --action-offline-mode` to start
without network checks for images and actions.

The Linux packages of `release.yml` pass files between jobs as artifacts,
which needs `act`'s artifact server:

```sh
act workflow_dispatch -W .github/workflows/release.yml -j linux \
    --artifact-server-path target/act/artifacts
```

`act` copies the working tree into the container, uncommitted changes
included, and leaves out what `.gitignore` ignores (so `target/` is not
copied). Each job builds from scratch inside its container; the
`Swatinem/rust-cache` step saves and restores `target/` through `act`'s own
cache server (`~/.cache/actcache`), so the second run of a job is faster.

## Debugging a job

```sh
act push -W .github/workflows/ci.yml -j test -v          # verbose: every command and its environment
act push -W .github/workflows/ci.yml -j test --reuse     # keep the container after the run
docker ps -a --filter name=act-CI                        # find it
docker exec -it <container> bash                         # the tree is copied to the same path as on the host
act push -W .github/workflows/ci.yml -j test --env RUST_BACKTRACE=1
```

Remove kept containers with `docker rm -f $(docker ps -aq --filter name=act-)`.

## Differences from GitHub's runners

- **Root user.** Jobs run as `root`, so file permissions are not enforced. A
  test that needs a read-only directory checks whether writing is refused
  and skips itself when it is not (see `fsio::tests` in `apps/cli`).
- **Linux only.** The Windows and macOS legs of `test` and the release
  packaging jobs for those systems do not run; they stay on GitHub.
- **`CI` is set.** As on GitHub, `opencircuit test --update-golden` is
  refused inside a job.
- **Smaller image.** The `act-*` images carry fewer preinstalled tools than
  GitHub's. Every job of `ci.yml` installs what it needs
  (`dtolnay/rust-toolchain` installs `rustup`), so this does not matter for
  CI; the Linux packaging job of `release.yml` installs its own tools too.
- **Other architectures.** `--container-architecture linux/arm64` runs the
  jobs under QEMU emulation when `binfmt` handlers are installed
  (`docker run --privileged --rm tonistiigi/binfmt --install arm64`). It is
  slow (a full build takes many times longer) and is not part of the
  default run.
