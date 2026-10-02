#![cfg(feature = "ffi")]
use grazer::{
    DrawSprite, Input,
    advanced::LaserSegment,
    ffi::*,
    game::{AdvancedHud, AudioEvent, GameSprite, Hud},
    resources::{SoundAsset, SpriteAsset},
};
use std::mem::{offset_of, size_of};
#[test]
fn stable_c_pod_sizes_offsets_and_version_families() {
    assert_eq!(
        (size_of::<GrazerConfig>(), offset_of!(GrazerConfig, seed)),
        (32, 24)
    );
    assert_eq!(
        (
            size_of::<GrazerGameConfig>(),
            offset_of!(GrazerGameConfig, seed)
        ),
        (24, 8)
    );
    assert_eq!(size_of::<GrazerBullet>(), 24);
    assert_eq!(size_of::<DrawSprite>(), 20);
    assert_eq!(size_of::<Input>(), 8);
    assert_eq!(size_of::<GrazerGameInput>(), 12);
    assert_eq!(
        (
            size_of::<GameSprite>(),
            offset_of!(GameSprite, x),
            offset_of!(GameSprite, rgba)
        ),
        (40, 16, 32)
    );
    assert_eq!(
        (size_of::<AudioEvent>(), offset_of!(AudioEvent, tick)),
        (16, 8)
    );
    assert_eq!(
        (
            size_of::<Hud>(),
            offset_of!(Hud, health),
            offset_of!(Hud, bomb_flash)
        ),
        (64, 24, 56)
    );
    assert_eq!(
        (size_of::<AdvancedHud>(), offset_of!(AdvancedHud, collected)),
        (48, 24)
    );
    assert_eq!(
        (size_of::<LaserSegment>(), offset_of!(LaserSegment, x1)),
        (40, 16)
    );
    assert_eq!(size_of::<SpriteAsset>(), 20);
    assert_eq!(size_of::<SoundAsset>(), 20);
    assert_eq!(size_of::<GrazerResourceInfo>(), 32);
    assert_eq!(size_of::<GrazerScriptDiagnostic>(), 32);
    assert_eq!(size_of::<GrazerReplayStatus>(), 48);
    assert_eq!(
        (
            grazer_abi_version(),
            grazer_game_abi_version(),
            grazer_script_api_version(),
            grazer_advanced_api_version(),
            grazer_checkpoint_api_version(),
            grazer_project_api_version()
        ),
        (1, 2, 1, 1, 1, 1)
    );
}
