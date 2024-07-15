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
pub const POPULATE_INDIRECT_BUFFER_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(2398076348923761);
pub const BATCH_FRUSTUM_CULL_HANDLE: Handle<Shader> = Handle::weak_from_u128(6547967980067631);
pub const DEFERRED_INDIRECT_CHUNK_HANDLE: Handle<Shader> = Handle::weak_from_u128(1209756888212873);

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
        POPULATE_INDIRECT_BUFFER_HANDLE,
        "populate_indirect_buffer.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(
        app,
        BATCH_FRUSTUM_CULL_HANDLE,
        "batch_frustum_cull.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(
        app,
        DEFERRED_INDIRECT_CHUNK_HANDLE,
        "deferred_indirect_chunk.wgsl",
        Shader::from_wgsl
    );
}
