# Neovide Documentation

<img src="docs/assets/neovide-128x128.png" alt="Neovide Logo" align="left"> This folder is the
source of documentation for Neovide, a simple no-nonsense, cross-platform, graphical editor for
Neovim, hosted on [neovide.dev](https://neovide.dev)

_Note: If you're looking for the Neovide source code, that can be found
[here](https://github.com/neovide/neovide)_

<br><br>

<div align="center">
    <img src="assets/landing-page.png" alt="Neovide Landing Page">
    <em>Screenshot of Landing Page</em>
</div>

## Description

- **./docs** is the source directory of the book
- **./docs/SUMMARY.md** configures the order of the menu on the left hand side of the book
  ([more info here](https://rust-lang.github.io/mdBook/format/summary.html)).
  - "Without this file, there is no book."
- **./docs/assets** stores all the non-markdown content (screenshots, gifs) to be used.
- **./theme** stores general files to be used by mdbook to configure the look and feel of the book
  ([more info here](https://rust-lang.github.io/mdBook/format/theme/index.html)).

### Build

1. Install the latest version of rust on your preferred choice of operating system, we recommend
   using [rustup](https://rustup.rs/)

2. Install required cargo packages to build successfully

   ```sh
   cargo install mdbook --version 0.4.21
   cargo install mdbook-pagetoc --version 0.1.4        # Table of Contents
   cargo install mdbook-linkcheck --version 0.7.6      # Check Links Aren't Broken
   ```

3. Clone the repository

   ```shell
   git clone https://github.com/neovide/neovide
   ```

4. Navigate to root directory

   ```shell
   cd neovide/website/
   ```

5. Build the book and open in default browser

   ```shell
   mdbook build --open
   ```

**Quick Tip!**

To see the website update in real-time while you edit the content, you can use `mdbook serve --open`
instead of the `build` argument

_Note: When making updates to any **.css/.js** files, a full refresh of the browser is needed. This
can be done by pressing <kbd>Ctrl</kbd>-<kbd>F5</kbd>_

## Contributing

First of all, thank you for your interest in wanting to contribute to the website! The documentation
is very much community-driven so your changes are very much appreciated!

A few things to note:

- This repository makes use of [mdbook](https://rust-lang.github.io/mdBook/index.html), it is
  _highly_ recommended to read up on mdbook's documentation before contributing. This will save you
  a lot of time debugging if you're messing around with the file structure!
- The markdown is linted to ensure that the style stays consistent, while we are _not_ going to
  outright reject any contributions if our CI fails, we very much encourage you to lint before
  raising a pull request as this will increase of your chances of getting the changes merged. The
  linter we use is [markdownlint](https://github.com/DavidAnson/markdownlint) and any custom rules
  that we have defined live in a [.markdownlintrc](.markdownlintrc). To save yourself the trouble of
  manually line wrapping, we recommend the usage of
  [mdformat](https://github.com/executablebooks/mdformat), the command
  `mdformat --wrap 100 --number` will ensure that you are in line with our CI.

Always feel free to ask questions by either joining our [discord](https://discord.gg/SjFpZdQys6) or
raising a GitHub issue!
