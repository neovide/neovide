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
  pointers or hints" or "I'm interested in contributing"), though that requires
  some flair. Even if the original reporter can't seem to solve the issue,
  someone else interested in contributing might lurk around and find exactly
  those pointers.

## How to release

### Preparing

- Head over to [the releases page][releases-page] and hit the `Draft a new
    release` button.
- Keep the resulting page somewhere safe open, you'll need to work with it the
    next half an hour and GitHub doesn't automatically save its contents.
- Create a new tag with an appropiate version number.

  We're not fully following [SemVer][semver] here, but as of 0.10.1 larger
  changes should be an increase in the MINOR part, while fixups should be an
  increase in the PATCH part.

- Hit the `Generate release notes` button.
- Reformat to be similar to previous releases

  - Rename the `What's Changed` section to `Changes`
  - Rewrite each line in the `Changes` section to reflect what this change means
      for the end user, linking to the relevant PR/commit
  - Group all bug fix PRs/commits under a point named `Bug fixes`
  - Have each line reflect what platform it applies to if not irrelevant

- Hit the `Save draft` button

You can make several rounds of preparing such releases through editing the
current draft over time, to make sure every contributor is mentioned and every
change is included.

[releases-page]: https://github.com/neovide/neovide/releases
[semver]: https://semver.org/

### Actually releasing

- Announce a short period of time where last changes to be done or fixup work
    can flow in (can be anything you imagine, though 24 hours to one week might
    be enough depending on the blocker)
- Wait for that period to pass
- Have a last look over the draft to make sure every new contributor and change has
    been mentioned

Now here's where the order becomes important:

- Make sure the working directory is clean
- Run `cargo update` and `cargo build`, make sure both succeed
- Create a commit named `Run cargo update` or similar
- Bump the version to match the tag name everywhere

  - `Cargo.toml`
  - `snap/snapcraft.yaml`
  - `website/docs/*.md` and update `Unreleased yet` to `Available since $tag`
      (where `$tag` is the tag name)

- Run `cargo build` and make sure it succeeds
- Create a commit called `Bump version to $tag`
- Push and wait for CI to complete (will take around 25 minutes)

In the meantime, you can look through the previous commits to see if you missed
anything.

- From the `Bump version to $tag` commit, download all the artifacts
- Unzip `neovide-linux.tar.gz.zip` to get `neovide.tar.gz`
- Head to the release draft, edit it and upload the produced artifacts (using
    the unzipped `neovide.tar.gz` for Linux)
- Hit `Publish release`
- profit

Phew. Now, announce the new release anywhere you think is appropiate (like
Reddit, Discord, whatever) ~~and go create a PR in nixpkgs~~.
