# winget

On Windows, pdvim installs with

```powershell
winget install djmbritt.pdvim
```

from the Windows Package Manager's community repository,
`microsoft/winget-pkgs`, which keeps each version as three manifests under
`manifests/d/djmbritt/pdvim/VERSION/`. `.github/workflows/winget.yml` makes
them for each published release with `manifest.sh` here, validates them,
installs pdvim from them on Windows and checks it starts, then opens a pull
request on `winget-pkgs` with `wingetcreate`. winget has the version once
Microsoft's checks pass and the pull request is merged, usually within a day
or two.

The package is portable: winget unpacks the release's `.zip` for Windows and
puts `pdvim` on the PATH, with no installer of its own. It asks for the
Visual C++ runtime, which Rust's MSVC builds link to.

## Setting it up, once

1. Make a classic personal access token (Settings > Developer settings >
   Personal access tokens > Tokens (classic)) with the `public_repo` scope.
   `wingetcreate` forks `microsoft/winget-pkgs` under the token's owner and
   opens the pull request from there.

2. In pdvim's repository, under Settings > Secrets and variables > Actions,
   add it as the secret `WINGET_TOKEN`.

3. Run "winget" from the Actions tab to send the latest release. The first
   pull request adds the package and is reviewed by hand; each release
   published after that is sent on its own.

## Trying it out

```sh
packaging/winget/manifest.sh 0.2.0 SHA256SUMS manifests
```

writes the manifests for a release, from its `SHA256SUMS`, to `manifests/`.
On Windows, install from them with

```powershell
winget settings --enable LocalManifestFiles   # once, as administrator
winget validate --manifest manifests
winget install --manifest manifests
```
