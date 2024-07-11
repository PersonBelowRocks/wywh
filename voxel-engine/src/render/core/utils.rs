use bevy::pbr::{MeshFlags, MeshPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::ShaderDefVal;
use bevy::render::Extract;

use crate::data::texture::GpuFaceTexture;
use crate::render::quad::GpuQuadBitfields;
use crate::util::ChunkSet;

#[derive(Resource, Default, Clone, Debug, Deref, DerefMut)]
pub struct InspectChunks(pub ChunkSet);

pub fn main_world_res_exists<T: Resource>(res: Extract<Option<Res<T>>>) -> bool {
    res.is_some()
}

pub fn u32_shader_def(name: &str, value: u32) -> ShaderDefVal {
    ShaderDefVal::UInt(name.into(), value)
}

pub fn add_shader_constants(shader_defs: &mut Vec<ShaderDefVal>) {
    let shader_constants = [
        u32_shader_def("ROTATION_MASK", GpuQuadBitfields::ROTATION_MASK),
        u32_shader_def("ROTATION_SHIFT", GpuQuadBitfields::ROTATION_SHIFT),
        u32_shader_def("FACE_MASK", GpuQuadBitfields::FACE_MASK),
        u32_shader_def("FACE_SHIFT", GpuQuadBitfields::FACE_SHIFT),
        u32_shader_def("FLIP_UV_X_BIT", GpuQuadBitfields::FLIP_UV_X_BIT),
        u32_shader_def("FLIP_UV_Y_BIT", GpuQuadBitfields::FLIP_UV_Y_BIT),
        u32_shader_def("HAS_NORMAL_MAP_BIT", GpuFaceTexture::HAS_NORMAL_MAP_BIT),
        u32_shader_def("HAS_NORMAL_MAP_BIT", GpuFaceTexture::HAS_NORMAL_MAP_BIT),
        u32_shader_def(
            "DEFAULT_PBR_INPUT_FLAGS",
            (MeshFlags::SHADOW_RECEIVER | MeshFlags::TRANSMITTED_SHADOW_RECEIVER).bits(),
        ),
    ];

    shader_defs.extend_from_slice(&shader_constants);
}

pub fn add_mesh_pipeline_shader_defs(key: MeshPipelineKey, shader_defs: &mut Vec<ShaderDefVal>) {
    if cfg!(feature = "pbr_transmission_textures") {
        shader_defs.push("PBR_TRANSMISSION_TEXTURES_SUPPORTED".into());
    }

    if key.msaa_samples() > 1 {
        shader_defs.push("MULTISAMPLED".into());
    };

    if key.contains(MeshPipelineKey::DEPTH_CLAMP_ORTHO) {
        shader_defs.push("DEPTH_CLAMP_ORTHO".into());
    }

    if key.contains(MeshPipelineKey::SCREEN_SPACE_AMBIENT_OCCLUSION) {
        shader_defs.push("SCREEN_SPACE_AMBIENT_OCCLUSION".into());
    }

    if key.contains(MeshPipelineKey::NORMAL_PREPASS) {
        shader_defs.push("NORMAL_PREPASS".into());
    }

    if key.contains(MeshPipelineKey::DEPTH_PREPASS) {
        shader_defs.push("DEPTH_PREPASS".into());
    }

    if key.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS) {
        shader_defs.push("MOTION_VECTOR_PREPASS".into());
    }

    if key.contains(MeshPipelineKey::DEFERRED_PREPASS) {
        shader_defs.push("DEFERRED_PREPASS".into());
    }

    if key.contains(MeshPipelineKey::NORMAL_PREPASS) && key.msaa_samples() == 1 {
        shader_defs.push("LOAD_PREPASS_NORMALS".into());
    }

    let view_projection = key.intersection(MeshPipelineKey::VIEW_PROJECTION_RESERVED_BITS);
    if view_projection == MeshPipelineKey::VIEW_PROJECTION_NONSTANDARD {
        shader_defs.push("VIEW_PROJECTION_NONSTANDARD".into());
    } else if view_projection == MeshPipelineKey::VIEW_PROJECTION_PERSPECTIVE {
        shader_defs.push("VIEW_PROJECTION_PERSPECTIVE".into());
    } else if view_projection == MeshPipelineKey::VIEW_PROJECTION_ORTHOGRAPHIC {
        shader_defs.push("VIEW_PROJECTION_ORTHOGRAPHIC".into());
    }

    if key.contains(MeshPipelineKey::TONEMAP_IN_SHADER) {
        shader_defs.push("TONEMAP_IN_SHADER".into());

        shader_defs.push(ShaderDefVal::UInt(
            "TONEMAPPING_LUT_TEXTURE_BINDING_INDEX".into(),
            20,
        ));
        shader_defs.push(ShaderDefVal::UInt(
            "TONEMAPPING_LUT_SAMPLER_BINDING_INDEX".into(),
            21,
        ));

        let method = key.intersection(MeshPipelineKey::TONEMAP_METHOD_RESERVED_BITS);

        if method == MeshPipelineKey::TONEMAP_METHOD_NONE {
            shader_defs.push("TONEMAP_METHOD_NONE".into());
        } else if method == MeshPipelineKey::TONEMAP_METHOD_REINHARD {
            shader_defs.push("TONEMAP_METHOD_REINHARD".into());
        } else if method == MeshPipelineKey::TONEMAP_METHOD_REINHARD_LUMINANCE {
            shader_defs.push("TONEMAP_METHOD_REINHARD_LUMINANCE".into());
        } else if method == MeshPipelineKey::TONEMAP_METHOD_ACES_FITTED {
            shader_defs.push("TONEMAP_METHOD_ACES_FITTED ".into());
        } else if method == MeshPipelineKey::TONEMAP_METHOD_AGX {
            shader_defs.push("TONEMAP_METHOD_AGX".into());
        } else if method == MeshPipelineKey::TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM {
            shader_defs.push("TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM".into());
        } else if method == MeshPipelineKey::TONEMAP_METHOD_BLENDER_FILMIC {
            shader_defs.push("TONEMAP_METHOD_BLENDER_FILMIC".into());
        } else if method == MeshPipelineKey::TONEMAP_METHOD_TONY_MC_MAPFACE {
            shader_defs.push("TONEMAP_METHOD_TONY_MC_MAPFACE".into());
        }

        // Debanding is tied to tonemapping in the shader, cannot run without it.
        if key.contains(MeshPipelineKey::DEBAND_DITHER) {
            shader_defs.push("DEBAND_DITHER".into());
        }
    }

    if key.contains(MeshPipelineKey::MAY_DISCARD) {
        shader_defs.push("MAY_DISCARD".into());
    }

    if key.contains(MeshPipelineKey::ENVIRONMENT_MAP) {
        shader_defs.push("ENVIRONMENT_MAP".into());
    }

    if key.contains(MeshPipelineKey::IRRADIANCE_VOLUME) {
        shader_defs.push("IRRADIANCE_VOLUME".into());
    }

    if key.contains(MeshPipelineKey::LIGHTMAPPED) {
        shader_defs.push("LIGHTMAP".into());
    }

    if key.contains(MeshPipelineKey::TEMPORAL_JITTER) {
        shader_defs.push("TEMPORAL_JITTER".into());
    }

    let shadow_filter_method =
        key.intersection(MeshPipelineKey::SHADOW_FILTER_METHOD_RESERVED_BITS);
    if shadow_filter_method == MeshPipelineKey::SHADOW_FILTER_METHOD_HARDWARE_2X2 {
        shader_defs.push("SHADOW_FILTER_METHOD_HARDWARE_2X2".into());
    } else if shadow_filter_method == MeshPipelineKey::SHADOW_FILTER_METHOD_GAUSSIAN {
        shader_defs.push("SHADOW_FILTER_METHOD_GAUSSIAN".into());
    } else if shadow_filter_method == MeshPipelineKey::SHADOW_FILTER_METHOD_TEMPORAL {
        shader_defs.push("SHADOW_FILTER_METHOD_TEMPORAL".into());
    }

    let blur_quality =
        key.intersection(MeshPipelineKey::SCREEN_SPACE_SPECULAR_TRANSMISSION_RESERVED_BITS);

    shader_defs.push(ShaderDefVal::Int(
        "SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS".into(),
        match blur_quality {
            MeshPipelineKey::SCREEN_SPACE_SPECULAR_TRANSMISSION_LOW => 4,
            MeshPipelineKey::SCREEN_SPACE_SPECULAR_TRANSMISSION_MEDIUM => 8,
            MeshPipelineKey::SCREEN_SPACE_SPECULAR_TRANSMISSION_HIGH => 16,
            MeshPipelineKey::SCREEN_SPACE_SPECULAR_TRANSMISSION_ULTRA => 32,
            _ => unreachable!(), // Not possible, since the mask is 2 bits, and we've covered all 4 cases
        },
    ));

    shader_defs.push("MULTIPLE_LIGHT_PROBES_IN_ARRAY".into());
    shader_defs.push("IRRADIANCE_VOLUMES_ARE_USABLE".into());
}
