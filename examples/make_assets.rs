fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "web/assets/demo".into()),
    );
    std::fs::create_dir_all(&path)?;
    std::fs::write(
        path.join("project.json"),
        grazer::resources::ResourcePack::builtin_manifest(),
    )?;
    std::fs::write(
        path.join("sprites.rgba"),
        grazer::resources::ResourcePack::builtin().atlas(),
    )?;
    println!("Wrote authored resource pack to {}", path.display());
    Ok(())
}
