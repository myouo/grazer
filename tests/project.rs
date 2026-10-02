use grazer::{
    GameConfig, GameInput,
    checkpoint::CheckpointError,
    language::{Program, VmLimits},
    project::{Project, ProjectError},
    resources::ResourcePack,
};
use std::sync::Arc;
#[test]
fn self_contained_project_preserves_full_game_resources_and_checkpoint_identity() {
    let pack = ResourcePack::builtin();
    let mut atlas = pack.atlas().to_vec();
    atlas[0] ^= 1;
    let custom = ResourcePack::new(
        1,
        pack.width(),
        pack.height(),
        atlas,
        pack.sprites().to_vec(),
        pack.sounds().to_vec(),
    )
    .unwrap();
    let p = Project::new(
        "External game",
        GameConfig::default(),
        123,
        custom,
        Arc::new(
            Program::compile(
                "external.graze",
                "task main() { drop(player(),1,1); while true { wave(random()%1000); wait(10); } }",
            )
            .unwrap(),
        ),
        VmLimits::default(),
        None,
    )
    .unwrap();
    let mut a = p.create_game().unwrap();
    let encoded = p.to_bytes().unwrap();
    let loaded = Project::from_bytes(&encoded).unwrap();
    assert_eq!(loaded.name(), "External game");
    assert_eq!(loaded.to_bytes().unwrap(), encoded);
    let mut b = loaded.create_game().unwrap();
    for _ in 0..1000 {
        let input = GameInput {
            fire: true,
            focus: true,
            ..Default::default()
        };
        a.step(input).unwrap();
        b.step(input).unwrap();
        assert_eq!(a.state_hash(), b.state_hash());
    }
    let restored = grazer::Game::<grazer::language::ScriptStage>::restore_checkpoint(
        loaded.resource_pack(),
        &a.checkpoint().unwrap(),
    )
    .unwrap();
    assert_eq!(restored.state_hash(), b.state_hash());
    assert_eq!(
        loaded.resources().content_hash(),
        p.resources().content_hash()
    );
}
#[test]
fn project_and_resource_archive_validate_versions_corruption_and_names() {
    let p = Project::showcase().unwrap();
    let bytes = p.to_bytes().unwrap();
    for n in [0, 8, 32, bytes.len() - 1] {
        assert!(Project::from_bytes(&bytes[..n]).is_err());
    }
    let mut bad = bytes.clone();
    bad[20] ^= 1;
    assert!(matches!(
        Project::from_bytes(&bad),
        Err(ProjectError::Archive(CheckpointError::Fingerprint))
    ));
    let pack = ResourcePack::builtin();
    let bytes = pack.to_bytes().unwrap();
    let restored = ResourcePack::from_bytes(&bytes).unwrap();
    assert_eq!(restored.content_hash(), pack.content_hash());
    let mut bad = bytes;
    bad[8] = 99;
    let end = bad.len() - 8;
    let mut h = 0xcbf29ce484222325u64;
    for &b in &bad[..end] {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    bad[end..].copy_from_slice(&h.to_le_bytes());
    assert!(matches!(
        ResourcePack::from_bytes(&bad),
        Err(CheckpointError::Version)
    ));
    assert!(
        Project::new(
            "",
            GameConfig::default(),
            42,
            pack,
            Arc::new(Program::compile("test.graze", "task main() { wait(1); }").unwrap()),
            VmLimits::default(),
            None
        )
        .is_err()
    );
}
