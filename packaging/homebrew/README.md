# The Homebrew tap

On macOS, pdvim installs with

```sh
brew install djmbritt/pdvim/pdvim
```

from the tap `djmbritt/homebrew-pdvim`, a repository of its own that holds
one file, `Casks/pdvim.rb`. `.github/workflows/homebrew.yml` writes that
file for each published release with `cask.sh` here, after installing it on
an Apple silicon and an Intel Mac and checking pdvim starts.

It is a cask rather than a formula because Homebrew lets only casks install
a different prebuilt download for each kind of processor. It installs the
program and its manual pages from the release's `.tar.gz` for macOS. Linux
has its own packages, which suit it better.

The program is not notarized by Apple, so macOS would refuse to run it the
first time. The cask takes the quarantine mark off the files it installs,
as taps for unsigned programs commonly do. Notarizing the release with an
Apple Developer account would make that unnecessary.

## Setting it up, once

The tap must be public for `brew` to fetch it, and the release's files must
be public for `brew` to download them, so this works once pdvim's own
repository is public.

1. Make the tap: a public repository named `homebrew-pdvim` under
   `djmbritt`, with a README. The name must start with `homebrew-`.

2. Make a fine-grained personal access token (Settings > Developer settings >
   Personal access tokens > Fine-grained tokens) for that repository alone,
   with the permission Contents: Read and write.

3. In pdvim's repository, under Settings > Secrets and variables > Actions,
   add it as the secret `HOMEBREW_TAP_TOKEN`.

4. Run "Homebrew" from the Actions tab to put the latest release in the
   tap. Each release published after that updates it on its own.

## Trying it out

```sh
packaging/homebrew/cask.sh 0.1.31 SHA256SUMS > pdvim.rb
```

prints the cask for a release, from its `SHA256SUMS`. Given a third
argument, as `file:///path/to/the/release`, it takes the packages from
there instead of from GitHub. Put it in a tap of your own to install it:

```sh
brew tap-new --no-git local/pdvim
cp pdvim.rb "$(brew --repo local/pdvim)/Casks/pdvim.rb"
brew install --cask local/pdvim/pdvim
```
