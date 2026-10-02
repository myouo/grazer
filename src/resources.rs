//! Versioned RGBA atlas and tone resources. Loading allocates; simulation only
//! refers to numeric IDs. JSON/filesystem loading is behind `resources`.
mod builtin;
pub use builtin::{FONT_CHARACTERS, FONT_ID_BASE};
pub const RESOURCE_VERSION: u32 = 1;
pub const PLAYER: u32 = 1;
pub const ENEMY: u32 = 2;
pub const BOSS: u32 = 3;
pub const PLAYER_SHOT: u32 = 4;
pub const ENEMY_SHOT: u32 = 5;
pub const HEART: u32 = 6;
pub const BOMB: u32 = 7;
pub const STAR: u32 = 8;
pub const SOLID: u32 = 9;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceError {
    Version,
    Atlas,
    Sprite(u32),
    Sound(u32),
    Duplicate(u32),
    Manifest(String),
    Io(String),
}
impl std::fmt::Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version => f.write_str("unsupported resource version"),
            Self::Atlas => f.write_str(
                "atlas must be 1..4096 pixels per axis with exactly width*height*4 RGBA bytes",
            ),
            Self::Sprite(id) => write!(f, "invalid or missing sprite resource {id}"),
            Self::Sound(id) => write!(f, "invalid or missing sound resource {id}"),
            Self::Duplicate(id) => write!(f, "duplicate resource ID {id}"),
            Self::Manifest(message) => write!(f, "resource manifest: {message}"),
            Self::Io(message) => write!(f, "resource file: {message}"),
        }
    }
}
impl std::error::Error for ResourceError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
#[cfg_attr(feature = "resources", derive(serde::Deserialize))]
#[cfg_attr(feature = "resources", serde(deny_unknown_fields))]
pub struct SpriteAsset {
    pub id: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
#[cfg_attr(feature = "resources", derive(serde::Deserialize))]
#[cfg_attr(feature = "resources", serde(deny_unknown_fields))]
pub struct SoundAsset {
    pub id: u32,
    /// 0 square, 1 triangle, 2 sine. This is presentation data, not simulation.
    pub waveform: u32,
    pub frequency: u32,
    pub duration_ms: u32,
    pub gain_q8: u32,
}

#[derive(Clone)]
pub struct ResourcePack {
    width: u32,
    height: u32,
    atlas: Vec<u8>,
    sprites: Vec<SpriteAsset>,
    sounds: Vec<SoundAsset>,
    hash: u64,
}
impl ResourcePack {
    pub fn new(
        version: u32,
        width: u32,
        height: u32,
        atlas: Vec<u8>,
        mut sprites: Vec<SpriteAsset>,
        mut sounds: Vec<SoundAsset>,
    ) -> Result<Self, ResourceError> {
        if version != RESOURCE_VERSION {
            return Err(ResourceError::Version);
        }
        if width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || atlas.len() != width as usize * height as usize * 4
        {
            return Err(ResourceError::Atlas);
        }
        if sprites.len() > 4096 || sounds.len() > 256 {
            return Err(ResourceError::Manifest(
                "resource count exceeds limits".into(),
            ));
        }
        sprites.sort_by_key(|s| s.id);
        sounds.sort_by_key(|s| s.id);
        for sprite in &sprites {
            if sprite.id == 0
                || sprite.width == 0
                || sprite.height == 0
                || sprite
                    .x
                    .checked_add(sprite.width)
                    .is_none_or(|end| end > width)
                || sprite
                    .y
                    .checked_add(sprite.height)
                    .is_none_or(|end| end > height)
            {
                return Err(ResourceError::Sprite(sprite.id));
            }
        }
        for sound in &sounds {
            if sound.id == 0
                || sound.waveform > 2
                || !(20..=20000).contains(&sound.frequency)
                || !(1..=5000).contains(&sound.duration_ms)
                || sound.gain_q8 > 128
            {
                return Err(ResourceError::Sound(sound.id));
            }
        }
        for pair in sprites.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(ResourceError::Duplicate(pair[0].id));
            }
        }
        for pair in sounds.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(ResourceError::Duplicate(pair[0].id));
            }
        }
        let mut hash = Fingerprint::new();
        hash.u32(version);
        hash.u32(width);
        hash.u32(height);
        hash.bytes(&atlas);
        hash.u32(sprites.len() as u32);
        for s in &sprites {
            for v in [s.id, s.x, s.y, s.width, s.height] {
                hash.u32(v);
            }
        }
        hash.u32(sounds.len() as u32);
        for s in &sounds {
            for v in [s.id, s.waveform, s.frequency, s.duration_ms, s.gain_q8] {
                hash.u32(v);
            }
        }
        Ok(Self {
            width,
            height,
            atlas,
            sprites,
            sounds,
            hash: hash.finish(),
        })
    }
    pub fn builtin() -> Self {
        Self::new(
            RESOURCE_VERSION,
            128,
            128,
            builtin::ATLAS.to_vec(),
            builtin::sprites(),
            builtin::sounds(),
        )
        .expect("authored resources are valid")
    }
    pub fn atlas(&self) -> &[u8] {
        &self.atlas
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn content_hash(&self) -> u64 {
        self.hash
    }
    pub fn sprites(&self) -> &[SpriteAsset] {
        &self.sprites
    }
    pub fn sounds(&self) -> &[SoundAsset] {
        &self.sounds
    }
    pub fn sprite(&self, id: u32) -> Option<&SpriteAsset> {
        self.sprites
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|i| &self.sprites[i])
    }
    pub fn sound(&self, id: u32) -> Option<&SoundAsset> {
        self.sounds
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|i| &self.sounds[i])
    }
    pub fn builtin_manifest() -> String {
        use std::fmt::Write;
        let mut out = String::from(
            "{\"version\":1,\"atlas\":{\"file\":\"sprites.rgba\",\"width\":128,\"height\":128},\"sprites\":[",
        );
        for (i, s) in builtin::sprites().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            write!(
                out,
                "{{\"id\":{},\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
                s.id, s.x, s.y, s.width, s.height
            )
            .expect("string write");
        }
        out.push_str("],\"sounds\":[");
        for (i, s) in builtin::sounds().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            write!(
                out,
                "{{\"id\":{},\"waveform\":{},\"frequency\":{},\"duration_ms\":{},\"gain_q8\":{}}}",
                s.id, s.waveform, s.frequency, s.duration_ms, s.gain_q8
            )
            .expect("string write");
        }
        out.push_str("]}\n");
        out
    }
    #[cfg(feature = "resources")]
    pub fn from_json(json: &str, atlas: Vec<u8>) -> Result<Self, ResourceError> {
        let manifest = Manifest::parse(json)?;
        Self::new(
            manifest.version,
            manifest.atlas.width,
            manifest.atlas.height,
            atlas,
            manifest.sprites,
            manifest.sounds,
        )
    }
    #[cfg(all(feature = "resources", not(target_arch = "wasm32")))]
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, ResourceError> {
        let path = path.as_ref();
        let json =
            std::fs::read_to_string(path).map_err(|error| ResourceError::Io(error.to_string()))?;
        let manifest = Manifest::parse(&json)?;
        let atlas = std::fs::read(
            path.parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join(&manifest.atlas.file),
        )
        .map_err(|error| ResourceError::Io(error.to_string()))?;
        Self::from_json(&json, atlas)
    }
}
#[cfg(feature = "resources")]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AtlasManifest {
    file: String,
    width: u32,
    height: u32,
}
#[cfg(feature = "resources")]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    atlas: AtlasManifest,
    sprites: Vec<SpriteAsset>,
    sounds: Vec<SoundAsset>,
}
#[cfg(feature = "resources")]
impl Manifest {
    fn parse(json: &str) -> Result<Self, ResourceError> {
        if json.len() > 1024 * 1024 {
            return Err(ResourceError::Manifest("manifest exceeds 1 MiB".into()));
        }
        let manifest: Self = serde_json::from_str(json)
            .map_err(|error| ResourceError::Manifest(error.to_string()))?;
        if manifest.version != RESOURCE_VERSION {
            return Err(ResourceError::Version);
        }
        if manifest.atlas.file.is_empty()
            || manifest.atlas.file.contains('\\')
            || manifest
                .atlas
                .file
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == ".." || part.contains(':'))
        {
            return Err(ResourceError::Manifest(
                "atlas file must be a relative path without parent components".into(),
            ));
        }
        Ok(manifest)
    }
}
pub(crate) struct Fingerprint(u64);
impl Fingerprint {
    pub fn new() -> Self {
        Self(0xcbf29ce484222325)
    }
    pub fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
    pub fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    pub fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    pub fn finish(self) -> u64 {
        self.0
    }
}
