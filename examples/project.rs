use grazer::{
    GameConfig,
    language::{Program, VmLimits},
    project::Project,
    resources::ResourcePack,
};
use std::sync::Arc;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str).unwrap_or("demo") {
        "demo" => {
            let path = args
                .get(2)
                .map(String::as_str)
                .unwrap_or("target/prism-passage.grazer");
            let p = Project::showcase()?;
            std::fs::write(path, p.to_bytes()?)?;
            println!("packed {} to {path}", p.name());
        }
        "pack" => {
            let source = args
                .get(2)
                .ok_or("pack SOURCE PROJECT_JSON OUTPUT [NAME]")?;
            let pack = ResourcePack::load(args.get(3).ok_or("project.json path")?)?;
            let program = Arc::new(Program::compile(source, &std::fs::read_to_string(source)?)?);
            let p = Project::new(
                args.get(5).map(String::as_str).unwrap_or("My game"),
                GameConfig::default(),
                42,
                pack,
                program,
                VmLimits::default(),
                None,
            )?;
            std::fs::write(args.get(4).ok_or("output path")?, p.to_bytes()?)?;
            println!("packed {}", p.name());
        }
        "inspect" => {
            let p = Project::from_bytes(&std::fs::read(args.get(2).ok_or("inspect FILE")?)?)?;
            println!(
                "name={} protocol={} program={:016x} resources={:016x}",
                p.name(),
                p.create_game()?.protocol_version(),
                p.program().content_hash(),
                p.resources().content_hash()
            );
        }
        _ => return Err("mode must be demo, pack or inspect".into()),
    }
    Ok(())
}
