use bevy::{
    app::App,
    asset::{embedded_asset, load_internal_asset, Handle},
    render::render_resource::Shader,
};

#[derive(Copy, Clone, Debug)]
pub struct ShaderStages<'a> {
    pub multidraw_vert: &'a str,
    pub multidraw_frag: &'a str,
    pub multidraw_prepass_vert: &'a str,
    pub multidraw_prepass_frag: &'a str,
}

pub static SHADER_STAGES: ShaderStages<'static> = ShaderStages {
    multidraw_vert: "embedded://voxel_engine/render/core/shaders/multidraw_chunk_vert.wgsl",
    multidraw_frag: "embedded://voxel_engine/render/core/shaders/multidraw_chunk_frag.wgsl",
    multidraw_prepass_vert:
        "embedded://voxel-engine/render/core/shaders/multidraw_chunk_prepass_vert.wgsl",
    multidraw_prepass_frag:
        "embedded://voxel-engine/render/core/shaders/multidraw_chunk_prepass_frag.wgsl",
};

pub const CHUNK_IO_HANDLE: Handle<Shader> = Handle::weak_from_u128(2653624191855805);
pub const CONSTANTS_HANDLE: Handle<Shader> = Handle::weak_from_u128(9817776592569886);
pub const MULTIDRAW_CHUNK_BINDINGS_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(8749226681332121);
pub const PBR_INPUT_HANDLE: Handle<Shader> = Handle::weak_from_u128(7716846957697771);
pub const REGISTRY_BINDINGS_HANDLE: Handle<Shader> = Handle::weak_from_u128(8499327436868843);
pub const TYPES_HANDLE: Handle<Shader> = Handle::weak_from_u128(1378018199763387);
pub const UTILS_HANDLE: Handle<Shader> = Handle::weak_from_u128(4464360603291233);

/// Loads the built-in voxel engine shaders.
pub fn load_internal_shaders(app: &mut App) {
    // Reusable shader logic
    load_internal_asset!(app, CHUNK_IO_HANDLE, "chunk_io.wgsl", Shader::from_wgsl);
    load_internal_asset!(app, CONSTANTS_HANDLE, "constants.wgsl", Shader::from_wgsl);
    load_internal_asset!(
        app,
        MULTIDRAW_CHUNK_BINDINGS_HANDLE,
        "multidraw_chunk_bindings.wgsl",
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

    // Shader stages
    embedded_asset!(app, "multidraw_chunk_vert.wgsl");
    embedded_asset!(app, "multidraw_chunk_frag.wgsl");
    embedded_asset!(app, "multidraw_chunk_prepass_vert.wgsl");
    embedded_asset!(app, "multidraw_chunk_prepass_frag.wgsl");
}
