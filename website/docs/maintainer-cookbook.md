# Maintainer Cookbook

General notes about collaborating/doing maintenance work on Neovide.

## How to keep your sanity

- Don't think you need to solve, participate in or even notice everything
    happening.

  Work on such a project where most things are already done and the things left
  aren't that fun anymore can be very gruesome, even if that might not be
  directly noticeable. Just do whatever is fun, feels doable and is inspiring
  you. This is not a full-time job, it's not even a job at all. You're not
  required to answer if you don't feel like doing so or would be forcing
  yourself.

  In short: Do whatever you seriously want.

- Always assume the best. There's no reason to be rude.

  Communication is hard. Even if it might seem like someone seriously didn't
  take any look at the docs before opening the issue, it's very possible that
  they did and found it not to be matching their case, misinterpreted what's
  written, or weren't sure if they were looking at the right section. What might
  feel obvious to you could feel obscure to another person.

  Re-state the essential docs contents, link to the relevant section and ask how
  it could be worded better/could be found better.

- Ask for more information if you require so. Some investigation can be done by
    the user.

  If some case requires some special environmental information which isn't given
  in the original report, ask for it. Or if you aren't sure what you're looking
  for, state what you believe is the case and add some potentially useful
  queries. It's also completely okay to state afterwards that you still don't
  have a clue.

  Neovide is a frontend for an arcane text editor, it's very possible that the
  person reporting the issue has some Rust or general programming knowledge and
  could help with debugging/tracing down the original cause for an issue. Some
  people state so if they want to do that (usually by "I'd be happy for any
  pointers or hints" or "I'm interested in contributing"), but this is more art
  than science. Even if the original reporter can't seem to solve the issue,
  someone else interested in contributing might lurk around and find exactly
  those pointers.

## How to release

Note: These are not a strict rulebook, but rather one _possible_ way for releasing. Adjust as you
see fit (and then update here with your findings).

### Preparing

1. Pick a new tag name with an appropriate version.

    We're not fully following [SemVer][semver] here, but as of 0.10.1 larger
    changes should be an increase in the MINOR part, while fixups should be an
    increase in the PATCH part.

2. Announce a short period of time where last changes to be done or fixup work
    can flow in (can be anything you imagine, though 24 hours to one week might
    be enough depending on the blocker).
3. Wait for that period to pass.

4. Have a last look over the draft to make sure every new contributor and change has
    been mentioned

[release-draft-workflow]: https://github.com/neovide/neovide/actions/workflows/release-draft.yml
[semver]: https://semver.org/

### Actually releasing

Now here's where the order becomes important:

1. Make sure the working directory is clean
2. Run `cargo update` and `cargo build`, make sure both succeed
3. Create a commit named `Run cargo update` or similar
4. Bump the version to match the tag name everywhere

    - `Cargo.toml` (do note it contains the version _twice_, one time in the
        top, one time at the bottom in the bundling section)
    - `extra/osx/Neovide.app/Contents/Info.plist`
    - `website/docs/*.md` and update `Nightly` to `Available since $tag`
      (where `$tag` is the tag name)

5. Run `cargo build` and make sure it succeeds, **remember to `git add
  Cargo.lock` to make sure releases stay reproducible
  ([#1628](https://github.com/neovide/neovide/issues/1628),
  [#1482](https://github.com/neovide/neovide/issues/1482))**
6. Check that `cargo publish --workspace --dry-run` works
   If necessary, you might have to bump the version number for the
   `neovide-derive` crate as well.
7. Create a commit called `Bump version to $tag`
8. Push and wait for CI to complete (will take around 25 minutes)
9. Run `cargo build --frozen`
10. Record the full commit SHA for the `Bump version to $tag` commit:

    ```sh
    target="$(git rev-parse HEAD)"
    ```

11. Create and push an annotated `$tag` tag that points at that exact commit:

    ```sh
    git tag -a "$tag" "$target" -m "$tag"
    git push origin "$tag"
    ```

In the meantime, you can look through the previous commits to see if you missed
anything.

1. Run the [Release Draft workflow][release-draft-workflow] with:

    - `tag`: the stable tag, for example `0.17.0`
    - `target`: the full commit SHA for the `Bump version to $tag` commit

2. Wait for the workflow to finish. It verifies the pushed annotated tag, builds
    the release artifacts, creates a GitHub draft release, generates release
    notes and uploads:

    - `neovide.msi`
    - `neovide-windows-x86_64.zip`
    - `neovide.AppImage`
    - `neovide.AppImage.zsync`
    - `neovide-linux-x86_64.tar.gz`
    - `Neovide-x86_64-apple-darwin.dmg`
    - `Neovide-aarch64-apple-darwin.dmg`

3. Head to the release draft and reformat the generated release notes to be
    similar to previous releases:

    - Rename the `What's Changed` section to `Changes`
    - Rewrite each line in the `Changes` section to reflect what this change means
      for the end user, linking to the relevant PR/commit
    - Group all bug fix PRs/commits under a point named `Bug fixes`
    - Have each line reflect what platform it applies to if not irrelevant

    You can make several rounds of preparing such releases through editing the
    current draft over time, to make sure every contributor is mentioned and every
    change is included.

4. Hit `Publish release`
5. Publish `neovide-derive` to crates.io if necessary `cargo publish -p neovide-derive`
6. Publish `neovide` to crates.io `cargo publish -p neovide`
7. profit

Phew. Now, announce the new release anywhere you think is appropriate (like
Reddit, Discord, whatever) ~~and go create a PR in nixpkgs~~.
