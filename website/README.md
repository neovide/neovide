## Build

To build neovide’s website, ensure you have [zola](https://getzola.org/) and `awk` installed.

Then, in this directory, run:
```sh
git submodule update --init
make serve
```

## Edit

To ease maintenance, markdown files in [`content`](./content/) are often sourced from other parts of the repository, thanks to [RISS](https://cj.rs/riss). RISS allows to change the files slightly, see the [transformation reference](https://cj.rs/readme-in-static-site/#transformations-reference) for the syntax.

For instance, `content/_index.md` is sourced from the [`README`](/README.md). Thus to make changes to `content/_index.md`, edit the [`README`](/README.md) and run:
```
make md_update
```
