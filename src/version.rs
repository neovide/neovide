pub const BUILD_VERSION: &str = env!("NEOVIDE_BUILD_VERSION");

#[cfg(target_os = "macos")]
pub fn release_channel() -> &'static str {
    release_channel_for_build_version(BUILD_VERSION)
}

#[cfg(any(target_os = "macos", test))]
fn release_channel_for_build_version(build_version: &str) -> &'static str {
    if build_version.starts_with("nightly-") { "nightly" } else { "stable" }
}

#[cfg(test)]
mod tests {
    use super::release_channel_for_build_version;

    #[test]
    fn release_channel_detects_nightly_builds() {
        assert_eq!(release_channel_for_build_version("nightly-104+g438415298449"), "nightly");
        assert_eq!(release_channel_for_build_version("nightly-104+g438415298449-dirty"), "nightly");
    }

    #[test]
    fn release_channel_defaults_to_stable() {
        assert_eq!(release_channel_for_build_version("0.15.2"), "stable");
        assert_eq!(release_channel_for_build_version("0.15.2-dirty"), "stable");
    }
}
