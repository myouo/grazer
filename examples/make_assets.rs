fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "web/assets/demo".into()),
    );
    std::fs::create_dir_all(&path)?;
    std::fs::write(
        path.join("advanced_showcase.graze"),
        include_str!("../assets/demo/advanced_showcase.graze"),
    )?;
    let examples = path.join("examples");
    std::fs::create_dir_all(&examples)?;
    for (name, source) in [
        ("ring", include_str!("../assets/examples/ring.graze")),
        ("fan", include_str!("../assets/examples/fan.graze")),
        ("aimed", include_str!("../assets/examples/aimed.graze")),
        ("spiral", include_str!("../assets/examples/spiral.graze")),
        ("motion", include_str!("../assets/examples/motion.graze")),
        (
            "straight_laser",
            include_str!("../assets/examples/straight_laser.graze"),
        ),
        (
            "curve_laser",
            include_str!("../assets/examples/curve_laser.graze"),
        ),
        (
            "boss_phases",
            include_str!("../assets/examples/boss_phases.graze"),
        ),
        (
            "drops_score",
            include_str!("../assets/examples/drops_score.graze"),
        ),
    ] {
        std::fs::write(examples.join(format!("{name}.graze")), source)?;
    }
    std::fs::write(
        path.join("project.json"),
        grazer::resources::ResourcePack::builtin_manifest(),
    )?;
    std::fs::write(
        path.join("first_sortie.graze"),
        include_str!("../assets/demo/first_sortie.graze"),
    )?;
    std::fs::write(
        path.join("bad_type.graze"),
        include_str!("../tests/fixtures/bad_type.graze"),
    )?;
    std::fs::write(
        path.join("loop.graze"),
        include_str!("../tests/fixtures/loop.graze"),
    )?;
    std::fs::write(
        path.join("sprites.rgba"),
        grazer::resources::ResourcePack::builtin().atlas(),
    )?;
    println!("Wrote authored resource pack to {}", path.display());
    Ok(())
}
