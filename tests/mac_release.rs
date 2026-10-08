//! Opt-in network smoke: verifies an official release without installing it.
use craft_apps_manager::{
    files, installers,
    jobs::Job,
    macos_build,
    model::{Paths, Preferences, Release},
    network::Network,
    updates,
};

#[test]
#[ignore = "downloads an official GitHub DMG and runs macOS verification"]
fn official_wordcraft_release_verifies_without_installing() -> anyhow::Result<()> {
    let root = std::env::temp_dir().join(format!("craft-release-smoke-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root)?;
    let result = (|| -> anyhow::Result<()> {
        let paths = Paths::new(root.clone(), None);
        let network = Network::new(&root)?;
        let release: Release =
            network.json("https://api.github.com/repos/storytold/wordcraft/releases/latest")?;
        let asset = updates::select_asset(&release, "wordcraft", &Preferences::default())?;
        let job = Job::new(paths.at("smoke.log"), &Default::default());
        let image = paths.at("official.dmg");
        network.asset(asset, &image, &job)?;
        let bundle = installers::extract_app(&image, &paths.at("extracted"), "wordcraft", &job)?;
        let record = macos_build::inspect(&bundle, "wordcraft")?;
        println!(
            "Verified official {}: {}, digest {}, native bundle {}",
            release.tag_name,
            asset.name,
            files::hash(&image)?,
            record.version
        );
        Ok(())
    })();
    std::fs::remove_dir_all(root)?;
    result
}
