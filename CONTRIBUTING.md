# Contributing

Thanks for helping. Issues and pull requests are welcome.

1. Open an issue first for anything larger than a small fix, so we can agree on the approach.
2. Set up the project as described under **Development** below.
3. Follow [CLAUDE.md](CLAUDE.md): logic in services, an access rule on every service, audit every change, and keep every control in [SECURITY.md](SECURITY.md) intact.
4. Before opening a pull request run `pnpm lint && pnpm typecheck && pnpm test`.
5. Add a line under **Unreleased** in `CHANGELOG.md` for anything a user or self-hoster would notice.

Schema changes need a new migration in `crates/db/migrations`; never edit one that has been released. Security issues: please report them privately through GitHub's security advisories, not in a public issue.

By contributing you agree that your contribution is licensed under the MIT license.

## Development

```sh
pnpm install
pnpm infra:up            # Postgres, S3, Mailpit and Gotenberg in Docker
cp .env.example .env
pnpm dev                 # server on :3000, web app on http://localhost:5173
```

| Command | |
| --- | --- |
| `pnpm lint` | Biome and Clippy |
| `pnpm typecheck` | The web app |
| `pnpm test` | Rust tests (set `TEST_DATABASE_URL`) and web tests |
| `pnpm api:types` | Regenerate the web app's typed client after changing a service |
| `docker compose --profile prod up -d --build` | The production image at http://localhost:8090 |

Email in development goes to Mailpit at http://localhost:8026.

## Releasing

Bump `version` in `Cargo.toml`, move the **Unreleased** notes in `CHANGELOG.md` under it, merge to `main`, then publish a GitHub Release named `vX.Y.Z`. CI runs every check, builds amd64 and arm64, and pushes `ghcr.io/nielsbosma/opensign:X.Y.Z`, `:X.Y` and `:latest`.

To deploy every release automatically, run `./deploy/Connect-Github.ps1` once (Windows, needs the GitHub CLI). It stores your Sliplane deploy hook as the `SLIPLANE_DEPLOY_HOOK` repository secret, and CI calls it after pushing the release image. The Sliplane service must run `:latest` or a release series such as `:0.1`.

On Windows, `./deploy/release.ps1` does the release and the deploy step by step: it checks that `main` is green, publishes the release with its changelog notes, follows the image build, then installs on Sliplane or calls the deploy hook. It saves the Sliplane token and deploy hook encrypted for your Windows account (`-ForgetCredentials` removes them).
