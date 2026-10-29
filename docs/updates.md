# Updates

Updating OpenSign is always the same thing: run a newer image. Database migrations run on their own when it starts.

## Knowing when to update

**Settings → System** shows:

- **Version**: the running version, the latest release on GitHub, and whether an update is available. "GitHub" links to the release notes.
- **Database**: the database version and the migrations that are applied.
- **Background Jobs**: emails, reminders, sealing and webhooks waiting or done in the last day, with recent failures.

Admins with an update waiting also see **Update to …** in the sidebar.

OpenSign asks GitHub for the latest release at most every six hours, and sends nothing but its version in the request's user agent. Turn this off with **Check for Updates** under **Updates** on the same page (for example, for installs without internet access).

The System page needs the **system.read** permission (admins and auditors). The same information is at `GET /api/v1/system/version` and `GET /api/v1/system/status`.

## Updating

### From the app (Deploy to Sliplane button)

Installs made with the **Deploy to Sliplane** button get an **Update Link** pinned from the deployer. When a release is out, **Update Now** on the System page opens the Sliplane Deployer with the new version. Sign in to Sliplane and confirm, and the deployer moves the services to the new image tag. Migrations run when the new version starts. Nothing needs to be set up, and no Sliplane credentials are stored in OpenSign.

### From the app (deploy hook)

1. In Sliplane, open the OpenSign service, then **Settings → Deploy hook**, and copy the hook URL.
2. In OpenSign, paste it as **Deploy Hook** under **Updates** on **Settings → System**.
3. Press **Update Now** on the System page. With both set, the deploy hook is used.

OpenSign calls the hook, Sliplane pulls the image again and restarts the service, and OpenSign is back on the new version a minute or two later. The hook redeploys the service's configured tag, so run `:latest` or a release series such as `:1.2`, not an exact version.

The deploy hook is stored encrypted, must be a public https address, and only an admin signed in to the app can trigger it. Every update is recorded in the audit trail.

### Sliplane on every release

If you build OpenSign from your own fork, CI can deploy each GitHub Release for you. Store the Sliplane deploy hook as the `SLIPLANE_DEPLOY_HOOK` repository secret (on Windows, `./deploy/Connect-Github.ps1` does this with the GitHub CLI). After CI pushes the release image, it calls the hook. Without the secret, nothing is deployed.

On Windows, `./deploy/release.ps1` publishes a release step by step: it checks that `main` is green, publishes the release with its changelog notes, follows the image build, then deploys.

### Sliplane from a terminal

```sh
SLIPLANE_TOKEN=api_rw_org_... OPENSIGN_TAG=1.2.0 ./deploy/sliplane.sh
```

### Docker Compose

```sh
docker compose pull && docker compose up -d
```

### Anywhere else

Change the image tag and redeploy, as your platform does it.

## Migrations

- Every start applies pending migrations before serving, under a database lock, so only one container migrates even when several start at once.
- Migrations only move forward. A build refuses to start against a database that a newer version already migrated, so an accidental downgrade cannot damage data.
- Back up the database before a major version.

## Release notes

Every release is listed with its changes on [GitHub Releases](https://github.com/nielsbosma/OpenSign/releases) and in `CHANGELOG.md`. Versions follow semantic versioning.
