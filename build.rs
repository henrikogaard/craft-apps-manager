fn main() {
    println!("cargo:rerun-if-changed=native/sparkle.m");
    println!("cargo:rerun-if-changed=native/dock.m");
    println!("cargo:rerun-if-changed=native/folder.m");
    println!("cargo:rerun-if-changed=native/login.m");
    println!("cargo:rerun-if-changed=native/hotkey.m");
    cc::Build::new()
        .file("native/sparkle.m")
        .file("native/dock.m")
        .file("native/folder.m")
        .file("native/login.m")
        .file("native/hotkey.m")
        .flag("-fobjc-arc")
        .flag("-fblocks")
        .compile("craft_sparkle");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=ServiceManagement");
    println!("cargo:rustc-link-lib=framework=Carbon");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    // GitHub repository (owner/name) that manager self-updates come from.
    // Forks can build against their own releases.
    println!("cargo:rerun-if-env-changed=CRAFT_MANAGER_REPOSITORY");
    let repository = std::env::var("CRAFT_MANAGER_REPOSITORY")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "henrikogaard/craft-apps-manager".into());
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    };
    match repository.split_once('/') {
        Some((owner, name)) if valid(owner) && valid(name) => {}
        _ => panic!("CRAFT_MANAGER_REPOSITORY must be owner/name"),
    }
    println!("cargo:rustc-env=CRAFT_MANAGER_REPOSITORY={repository}");
    let timestamp = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System clock before Unix epoch")
                .as_secs()
        });
    println!("cargo:rustc-env=CRAFT_BUILD_TIMESTAMP={timestamp}");
    println!(
        "cargo:rustc-env=CRAFT_BUILD_PROFILE={}",
        std::env::var("PROFILE").unwrap()
    );
    println!(
        "cargo:rustc-env=CRAFT_BUILD_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
}
