### OSX DMG Background

source.pdf in this directory can be used for modifying the background of the
dmg installer on OSX.

JetBrainsMono can be downloaded from [Nerd Fonts](https://www.nerdfonts.com/font-downloads).

After any changes, you must export 2 images with the following paths, filenames
and dimensions:

  1. `assets/neovide-dmg-background.png` (650x450)
  2. `assets/neovide-dmg-background@2x.png` (1300x900)

Next, to support retina images you must generate a properly composed tiff by running the builder script from the root of the project:

```
macos-builder/make-icns
```

This script will also generate an `.icns` file with all the proper sizes from
the `assets/neovide-1024.png` source image.
