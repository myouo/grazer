use super::{
    INVALID_ARGUMENT, OK,
    game::{GrazerGame, HostedGame},
    guard,
    replay::{REPLAY_DATA_ERROR, REPLAY_INCOMPATIBLE},
};
use crate::{
    checkpoint::CheckpointError,
    project::{MAX_PROJECT_BYTES, Project, ProjectError},
};
fn code(e: ProjectError) -> i32 {
    match e {
        ProjectError::Archive(
            CheckpointError::Version | CheckpointError::Stage | CheckpointError::Resource,
        ) => REPLAY_INCOMPATIBLE,
        _ => REPLAY_DATA_ERROR,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn grazer_project_api_version() -> u32 {
    1
}
/// # Safety
/// Source has length readable bytes; out is writable/aligned/nonoverlapping.
/// Source is borrowed for the call; successful output transfers unique ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_create_project(
    bytes: *const u8,
    length: u32,
    out: *mut *mut GrazerGame,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies writable output pointer storage.
        unsafe {
            *out = std::ptr::null_mut();
        }
        if bytes.is_null() || length as usize > MAX_PROJECT_BYTES {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller guarantees length readable source bytes.
        let bytes = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
        let project = match Project::from_bytes(bytes) {
            Ok(p) => p,
            Err(e) => return code(e),
        };
        match project.create_game() {
            Ok(game) => {
                // SAFETY: out is writable; caller owns the resulting Game handle.
                unsafe {
                    *out = Box::into_raw(Box::new(GrazerGame {
                        inner: HostedGame::Script(Box::new(game)),
                    }));
                }
                OK
            }
            Err(_) => REPLAY_DATA_ERROR,
        }
    })
}
/// # Safety
/// Both byte buffers are readable, output is writable/aligned/nonoverlapping.
/// Restores a scripted checkpoint using resources from the supplied project.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_restore_project(
    project: *const u8,
    project_length: u32,
    checkpoint: *const u8,
    checkpoint_length: u32,
    out: *mut *mut GrazerGame,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies writable output pointer storage.
        unsafe {
            *out = std::ptr::null_mut();
        }
        if project.is_null()
            || checkpoint.is_null()
            || project_length as usize > MAX_PROJECT_BYTES
            || checkpoint_length as usize > crate::checkpoint::MAX_CHECKPOINT_BYTES
        {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller guarantees both source buffer lengths/readability.
        let project_bytes = unsafe { std::slice::from_raw_parts(project, project_length as usize) };
        // SAFETY: caller guarantees checkpoint_length readable bytes.
        let bytes = unsafe { std::slice::from_raw_parts(checkpoint, checkpoint_length as usize) };
        let project = match Project::from_bytes(project_bytes) {
            Ok(p) => p,
            Err(e) => return code(e),
        };
        let info = match crate::game::checkpoint::CheckpointInfo::inspect(bytes) {
            Ok(i) => i,
            Err(e) => return code(e.into()),
        };
        if info.identity.content_hash != project.program().content_hash() {
            return REPLAY_INCOMPATIBLE;
        }
        match crate::Game::<crate::language::ScriptStage>::restore_checkpoint(
            project.resource_pack(),
            bytes,
        ) {
            Ok(game) => {
                // SAFETY: transfers unique ownership of the successful allocation.
                unsafe {
                    *out = Box::into_raw(Box::new(GrazerGame {
                        inner: HostedGame::Script(Box::new(game)),
                    }));
                }
                OK
            }
            Err(e) => code(e.into()),
        }
    })
}
