# Editing w/ External Tools

You can use Neovide in other programs as editor, this page aims to document some quirks. Support for
that, however, is only possible as far as reasonably debuggable.

_Note: We do not endorse nor disrecommend usage of all programs listed here. All usage happens on
your own responsibility._

## [jrnl](https://github.com/jrnl-org/jrnl)

In your configuration file:

```yaml
editor: "neovide --no-fork"
```

...as `jrnl` saves & removes the temporary file as soon as the main process exits, which happens
before startup by [forking](<https://en.wikipedia.org/wiki/Fork_(system_call)>).
