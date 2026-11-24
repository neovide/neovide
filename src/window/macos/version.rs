use objc2_foundation::NSOperatingSystemVersion;
use objc2_foundation::NSProcessInfo;

pub fn at_least(major: isize, minor: isize, patch: isize) -> bool {
    NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(NSOperatingSystemVersion {
        majorVersion: major,
        minorVersion: minor,
        patchVersion: patch,
    })
}
