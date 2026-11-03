# Website

The project website, published on GitHub Pages at
<https://brahm.github.io/desk/>. It is plain HTML and CSS, with no
build tool and no scripts.

`ci/build-site.sh` assembles it in `target/site`, copying the logo and
the app icons from [data](../data) so that they have one source. Open
`target/site/index.html` in a browser to see it. The
[Website workflow](../.github/workflows/pages.yml) builds it on every
pull request that touches it and publishes it from `main`.

## Translations

The page is written in English at the root, and each translation is a
copy in a directory named by its language code, as GNOME names its
translations: `pt-BR/index.html` for Brazilian Portuguese. Pages in a
directory reach the shared files with `../`. When you add a language:

- add it to the language menu in the header bar of every page, with
  `hreflang` and `lang`, and `aria-current="page"` on the page's own;
- add it to the `<link rel="alternate" hreflang="...">` list in the head
  of every page, where `x-default` stays English.

Those two lists are where a later step can pick the language of the
browser, and where they can be generated once the page moves to a
gettext template for every language that GNOME supports.

## Sharing the site with the Flatpak repository

The Flatpak repository of Desk is published on the same GitHub Pages
site, so the two share one address and one deployment. The pages must
not use these paths, and `ci/build-site.sh` fails if they do:

| Path | Content |
|---|---|
| `/repo/` | The OSTree repository, signed with GPG, with the nightly, beta and stable branches |
| `/desk.flatpakrepo` | The file that adds the repository: `flatpak remote-add --if-not-exists desk https://brahm.github.io/desk/desk.flatpakrepo` |
| `/*.flatpakref` | One file per app, to install it with one click |
| `/flatpak/` | Reserved for anything else the repository needs, such as the public key |

GitHub Pages replaces the whole site on each deployment, so the
repository cannot be uploaded on its own. The
[Flatpak workflow](../.github/workflows/flatpak.yml) keeps it between
runs as `flatpak-repo.tar`, an asset of the `flatpak-repo` release:
after each build from `main` or from a beta tag, it unpacks the last
copy, adds the build with `ci/flatpak-publish.sh`, uploads it again and
runs the Website workflow, which unpacks it into `_site` before the
upload step. Each branch keeps its last five builds, and the debug
symbols stay out, so that the site stays under the 1 GB that GitHub
Pages allows. After each deployment, the Website workflow installs the
app from `desk.flatpakrepo`, as people do.

The nightly branch is published every night. The beta branch is
published when a tag such as `v0.5.0-beta1` is pushed, or when the
Flatpak workflow is run by hand with the beta channel.

### The signing key

The repository is signed with a GPG key kept in the `FLATPAK_GPG_KEY`
secret of the GitHub repository, as an armored private key with no
passphrase. The workflow exports its public part into
`flatpak/desk.gpg` and into the `GPGKey` of the `.flatpakrepo` and
`.flatpakref` files. To create it, on a computer you trust:

```sh
export GNUPGHOME=$(mktemp -d)
gpg --batch --passphrase '' --quick-gen-key "Desk Flatpak repository" rsa4096 sign never
gpg --armor --export-secret-keys | gh secret set FLATPAK_GPG_KEY --repo brahm/desk
gpg --armor --export-secret-keys > desk-flatpak-key.asc   # keep it offline
rm -rf "$GNUPGHOME"
```

The key must not change once people have added the repository, since
Flatpak would refuse the updates signed with a new one. Keep the copy
offline, to put it back in the secret if it is lost.
