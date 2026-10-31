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
repository cannot be uploaded on its own. The workflow that builds the
Flatpaks has to add it to the same artifact as the pages: it keeps the
OSTree repository between runs (for example in a release or a branch of
its own), updates it with the new builds, and copies it into `_site`
before the upload step of the Website workflow. A site on GitHub Pages
is limited to 1 GB, so the repository has to prune old nightly builds.
