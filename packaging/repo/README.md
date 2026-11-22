# The apt and dnf repositories

`.github/workflows/package-repo.yml` publishes pdvim's `.deb` and `.rpm`
packages as an apt and a dnf repository on GitHub Pages, at
<https://djmbritt.github.io/pdvim/>, each time a release is published.
`packaging/repo.sh` makes them. The page there says how to add them.

GitHub Pages serves a private repository's site only on a paid plan, and
the site is public either way.

## Setting it up, once

1. Make a key to sign the repositories with. RSA rather than Ed25519, so
   that older rpm and apt can check it. It has no passphrase, as the
   workflow has nobody to type one; it is kept as a secret instead.

   ```sh
   export GNUPGHOME=$(mktemp -d)
   gpg --batch --passphrase '' --quick-gen-key \
       'pdvim packages <you@example.com>' rsa4096 sign 5y
   gpg --list-keys --with-colons | awk -F: '/^fpr/ { print $10; exit }'
   gpg --armor --export-secret-keys > pdvim-signing.asc
   ```

   The second command prints the key's fingerprint. Keep
   `pdvim-signing.asc` somewhere safe, away from the repository.

2. In the repository's Settings:
   - Secrets and variables > Actions > Secrets: add `REPO_SIGNING_KEY`,
     the contents of `pdvim-signing.asc`.
   - Secrets and variables > Actions > Variables: add `REPO_KEY_ID`, the
     fingerprint.
   - Pages > Build and deployment > Source: choose "GitHub Actions".

3. Publish a release, or run "Package repositories" from the Actions tab
   to publish the latest one.

Then delete the key's copy from `$GNUPGHOME` and from wherever else it was
made.

## Renewing or replacing the key

The key expires after five years. Extend it with `gpg --quick-set-expire`
and update both secrets before then. Users of a new key must fetch
`pdvim.gpg` or `pdvim.asc` again, as the page says, so keep the same key for
as long as it is safe.

## Trying it out

`repo.sh` needs `apt-ftparchive` (apt-utils), `createrepo_c` and `rpmsign`
(rpm). In an `ubuntu:24.04` container, with a key made as above and the
packages of a release in `packages/`:

```sh
REPO_KEY_ID=FINGERPRINT packaging/repo.sh packages site http://localhost:8000
python3 -m http.server -d site 8000
```

Then follow the page's steps with `http://localhost:8000` in a Debian,
Ubuntu, Fedora or openSUSE container on the same network.
