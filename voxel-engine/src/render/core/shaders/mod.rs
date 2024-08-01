use bevy::{
    app::App,
    asset::{load_internal_asset, Handle},
    render::render_resource::Shader,
};

pub const CHUNK_IO_HANDLE: Handle<Shader> = Handle::weak_from_u128(2653624191855805);
pub const CONSTANTS_HANDLE: Handle<Shader> = Handle::weak_from_u128(9817776592569886);
pub const INDIRECT_CHUNK_BINDINGS_HANDLE: Handle<Shader> = Handle::weak_from_u128(8749226681332121);
pub const PBR_INPUT_HANDLE: Handle<Shader> = Handle::weak_from_u128(7716846957697771);
pub const REGISTRY_BINDINGS_HANDLE: Handle<Shader> = Handle::weak_from_u128(8499327436868843);
pub const TYPES_HANDLE: Handle<Shader> = Handle::weak_from_u128(1378018199763387);
pub const UTILS_HANDLE: Handle<Shader> = Handle::weak_from_u128(4464360603291233);
pub const PREPROCESS_BATCH_HANDLE: Handle<Shader> = Handle::weak_from_u128(6547967980067631);
pub const PREPROCESS_LIGHT_BATCH_HANDLE: Handle<Shader> = Handle::weak_from_u128(2910271245758926);
pub const CHUNK_VERT_HANDLE: Handle<Shader> = Handle::weak_from_u128(1209756888212873);
pub const CHUNK_FRAG_HANDLE: Handle<Shader> = Handle::weak_from_u128(9210096709100541);
pub const CONSTRUCT_HZB_LEVEL_HANDLE: Handle<Shader> = Handle::weak_from_u128(2462168342385241);
pub const OCCLUDER_DEPTH_HANDLE: Handle<Shader> = Handle::weak_from_u128(1240701701561346);

/// Loads the built-in voxel engine shaders.
pub fn load_internal_shaders(app: &mut App) {
    // Reusable shader logic
    load_internal_asset!(app, CHUNK_IO_HANDLE, "chunk_io.wgsl", Shader::from_wgsl);
    load_internal_asset!(app, CONSTANTS_HANDLE, "constants.wgsl", Shader::from_wgsl);
    load_internal_asset!(
        app,
        INDIRECT_CHUNK_BINDINGS_HANDLE,
        "indirect_chunk_bindings.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(app, PBR_INPUT_HANDLE, "pbr_input.wgsl", Shader::from_wgsl);
    load_internal_asset!(
        app,
        REGISTRY_BINDINGS_HANDLE,
        "registry_bindings.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(app, TYPES_HANDLE, "types.wgsl", Shader::from_wgsl);
    load_internal_asset!(app, UTILS_HANDLE, "utils.wgsl", Shader::from_wgsl);
    load_internal_asset!(
        app,
        PREPROCESS_BATCH_HANDLE,
        "preprocess_batch.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(
        app,
        PREPROCESS_LIGHT_BATCH_HANDLE,
        "preprocess_light_batch.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(app, CHUNK_VERT_HANDLE, "chunk_vert.wgsl", Shader::from_wgsl);
    load_internal_asset!(app, CHUNK_FRAG_HANDLE, "chunk_frag.wgsl", Shader::from_wgsl);
    load_internal_asset!(
        app,
        CONSTRUCT_HZB_LEVEL_HANDLE,
        "construct_hzb_level.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(
        app,
        OCCLUDER_DEPTH_HANDLE,
        "occluder_depth.wgsl",
        Shader::from_wgsl
    );
}
