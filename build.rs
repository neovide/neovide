use std::process::Command;

fn main() {
    set_rerun();

    println!("cargo:rustc-env=NEOVIDE_BUILD_VERSION={}", build_version());

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/neovide.ico");
        res.compile().expect("Could not attach exe icon");
    }
}

fn set_rerun() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=assets/neovide.ico");

    for path in ["HEAD", "refs", "index", "packed-refs"] {
        if let Some(path) = git_path(path) {
            println!("cargo:rerun-if-changed={path}");
        }
    }

    if let Some(files) = git_output(&["ls-files", "-z"]) {
        for file in files.split('\0').filter(|file| !file.is_empty()) {
            println!("cargo:rerun-if-changed={file}");
        }
    } else {
        println!("cargo:warning=Could not list tracked files for version rerun tracking");
    }
}

fn build_version() -> String {
    let package_version = env!("CARGO_PKG_VERSION");
    let git_describe = git_output(&[
        "describe",
        "--tags",
        "--match",
        "[0-9]*.[0-9]*.[0-9]*",
        "--dirty",
        "--always",
    ]);

    git_describe
        .as_deref()
        .map(|describe| format(package_version, describe))
        .unwrap_or_else(|| package_version.to_owned())
}

fn format(version: &str, describe: &str) -> String {
    if describe == version {
        return version.to_owned();
    }

    if describe == format!("{version}-dirty") {
        return describe.to_owned();
    }

    let (describe, dirty) = match describe.strip_suffix("-dirty") {
        Some(version) => (version, true),
        None => (describe, false),
    };

    let mut parts = describe.splitn(3, '-');
    let tag = parts.next();
    let height = parts.next();
    let commit = parts.next();

    match (tag, height, commit) {
        // formats non-exact matches as <tag>-<count>-g<abbrev>
        // see: https://git-scm.com/docs/git-describe#_examples
        (Some(tag), Some(height), Some(commit)) if tag == version && commit.starts_with('g') => {
            let mut version = format!("nightly-{height}+{commit}");
            if dirty {
                version.push_str("-dirty");
            }
            version
        }
        _ => version.to_owned(),
    }
}

fn git_path(path: &str) -> Option<String> {
    git_output(&["rev-parse", "--git-path", path])
}

fn git_output(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim().to_owned())
}
