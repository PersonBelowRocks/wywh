use bevy::{
    pbr::{
        MaterialPipeline, MaterialPipelineKey, StandardMaterialFlags, PBR_PREPASS_SHADER_HANDLE,
    },
    prelude::*,
    reflect::{Reflect, TypeUuid},
    render::{
        mesh::MeshVertexBufferLayout,
        render_asset::RenderAssets,
        render_resource::{
            AsBindGroup, AsBindGroupShaderType, Face, RenderPipelineDescriptor, ShaderDefVal,
            ShaderRef, ShaderType, SpecializedMeshPipelineError,
        },
    },
};

use crate::render::mesh::ChunkMesh;

/// A material with "standard" properties used in PBR lighting
/// Standard property values with pictures here
/// <https://google.github.io/filament/Material%20Properties.pdf>.
///
/// May be created directly from a [`Color`] or an [`Image`].
#[derive(AsBindGroup, Asset, Reflect, Debug, Clone, TypeUuid, Default)]
#[uuid = "e65799f2-923e-4548-8879-be574f9db988"]
#[bind_group_data(VoxelChunkMaterialKey)]
#[uniform(0, VoxelChunkMaterialUniform)]
#[reflect(Default, Debug)]
pub struct VoxelChunkMaterial {}

/// The GPU representation of the uniform data of a [`VoxelChunkMaterial`].
#[derive(Clone, Default, ShaderType)]
pub struct VoxelChunkMaterialUniform {
    /// Doubles as diffuse albedo for non-metallic, specular for metallic and a mix for everything
    /// in between.
    pub base_color: Vec4,
    // Use a color for user friendliness even though we technically don't use the alpha channel
    // Might be used in the future for exposure correction in HDR
    pub emissive: Vec4,
    /// Linear perceptual roughness, clamped to [0.089, 1.0] in the shader
    /// Defaults to minimum of 0.089
    pub roughness: f32,
    /// From [0.0, 1.0], dielectric to pure metallic
    pub metallic: f32,
    /// Specular intensity for non-metals on a linear scale of [0.0, 1.0]
    /// defaults to 0.5 which is mapped to 4% reflectance in the shader
    pub reflectance: f32,
    /// The [`VoxelChunkMaterialFlags`] accessible in the `wgsl` shader.
    pub flags: u32,
    /// When the alpha mode mask flag is set, any base color alpha above this cutoff means fully opaque,
    /// and any below means fully transparent.
    pub alpha_cutoff: f32,
    /// The depth of the [`VoxelChunkMaterial::depth_map`] to apply.
    pub parallax_depth_scale: f32,
    /// In how many layers to split the depth maps for Steep parallax mapping.
    ///
    /// If your `parallax_depth_scale` is >0.1 and you are seeing jaggy edges,
    /// increase this value. However, this incurs a performance cost.
    pub max_parallax_layer_count: f32,
    /// Using [`ParallaxMappingMethod::Relief`], how many additional
    /// steps to use at most to find the depth value.
    pub max_relief_mapping_search_steps: u32,
}

impl AsBindGroupShaderType<VoxelChunkMaterialUniform> for VoxelChunkMaterial {
    fn as_bind_group_shader_type(
        &self,
        _images: &RenderAssets<Image>,
    ) -> VoxelChunkMaterialUniform {
        let mut flags = StandardMaterialFlags::NONE;
        flags |= StandardMaterialFlags::FOG_ENABLED;
        flags |= StandardMaterialFlags::ALPHA_MODE_OPAQUE;

        VoxelChunkMaterialUniform {
            base_color: Color::rgb(0.2, 0.22, 0.3).as_linear_rgba_f32().into(),
            emissive: Color::BLACK.as_linear_rgba_f32().into(),
            roughness: 1.0,
            metallic: 0.0,
            reflectance: 0.2,
            flags: flags.bits(),
            alpha_cutoff: 0.5,
            parallax_depth_scale: 0.1,
            max_parallax_layer_count: 16.0,
            max_relief_mapping_search_steps: parallax_mapping_method_max_steps(
                ParallaxMappingMethod::Occlusion,
            ),
        }
    }
}

pub fn parallax_mapping_method_max_steps(p: ParallaxMappingMethod) -> u32 {
    match p {
        ParallaxMappingMethod::Occlusion => 0,
        ParallaxMappingMethod::Relief { max_steps } => max_steps,
    }
}

/// The pipeline key for [`VoxelChunkMaterial`].
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct VoxelChunkMaterialKey {
    normal_map: bool,
    cull_mode: Option<Face>,
    depth_bias: i32,
    relief_mapping: bool,
}

impl From<&VoxelChunkMaterial> for VoxelChunkMaterialKey {
    fn from(_material: &VoxelChunkMaterial) -> Self {
        VoxelChunkMaterialKey {
            normal_map: false,
            cull_mode: Some(Face::Back),
            depth_bias: 0,
            relief_mapping: false,
        }
    }
}

macro_rules! uint_shader_def {
    ($label:ident) => {
        ShaderDefVal::UInt(stringify!($label).to_string(), $label)
    };
}

impl Material for VoxelChunkMaterial {
    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayout,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        use super::vertex::consts::*;

        let vertex_shader_defs = vec![
            ShaderDefVal::UInt("UINT_BITS".to_string(), u32::BITS),
            uint_shader_def!(FACE_BITMASK),
            uint_shader_def!(FACE_RSHIFT),
            uint_shader_def!(VXL_X_BITMASK),
            uint_shader_def!(VXL_X_RSHIFT),
            uint_shader_def!(VXL_Y_BITMASK),
            uint_shader_def!(VXL_Y_RSHIFT),
            uint_shader_def!(VXL_Z_BITMASK),
            uint_shader_def!(VXL_Z_RSHIFT),
            uint_shader_def!(TEX_ATLAS_X_BITMASK),
            uint_shader_def!(TEX_ATLAS_X_RSHIFT),
            uint_shader_def!(TEX_ATLAS_Y_BITMASK),
            uint_shader_def!(TEX_ATLAS_Y_RSHIFT),
            uint_shader_def!(CORNER_BITMASK),
            uint_shader_def!(CORNER_RSHIFT),
        ];

        let vertex_layout =
            layout.get_layout(&[ChunkMesh::VOXEL_DATA_ATTR.at_shader_location(0)])?;

        println!(
            "{:?}: {:?}\n",
            descriptor.label, descriptor.vertex.shader_defs
        );

        descriptor.vertex.buffers = vec![vertex_layout];
        descriptor
            .vertex
            .shader_defs
            .extend_from_slice(&vertex_shader_defs);
        descriptor.vertex.shader_defs.push("VERTEX_NORMALS".into());
        descriptor.vertex.shader_defs.push("VERTEX_COLORS".into());
        descriptor.vertex.shader_defs.push("NORMAL_PREPASS".into());

        if let Some(fragment) = descriptor.fragment.as_mut() {
            let shader_defs = &mut fragment.shader_defs;
            shader_defs.push("VERTEX_NORMALS".into());
            shader_defs.push("VERTEX_COLORS".into());
            shader_defs.push("NORMAL_PREPASS".into());

            if key.bind_group_data.normal_map {
                shader_defs.push("VoxelChunkMaterial_NORMAL_MAP".into());
            }
            if key.bind_group_data.relief_mapping {
                shader_defs.push("RELIEF_MAPPING".into());
            }

            println!("{:?}: {:?}\n", descriptor.label, shader_defs);
        }

        descriptor.primitive.cull_mode = key.bind_group_data.cull_mode;
        if let Some(label) = &mut descriptor.label {
            *label = format!("vxlpbr_{}", *label).into();
        }
        if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
            depth_stencil.bias.constant = key.bind_group_data.depth_bias;
        }
        Ok(())
    }

    // TODO: custom prepass shaders, will hopefully fix some issues
    // caused by the pbr function only being declared if the shader is
    // not in prepass
    // (https://github.com/bevyengine/bevy/blob/main/crates/bevy_pbr/src/render/pbr_functions.wgsl#L179C8-L179C8)
    //
    // See here for examples https://github.com/bevyengine/bevy/tree/main/crates/bevy_pbr/src/prepass
    fn prepass_vertex_shader() -> ShaderRef {
        "shaders/voxel_chunk_prepass.wgsl".into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        // PBR_PREPASS_SHADER_HANDLE.into()
        "shaders/voxel_chunk_frag_prepass.wgsl".into()
    }

    fn vertex_shader() -> ShaderRef {
        "shaders/voxel_chunk_vertex.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/voxel_chunk_frag.wgsl".into()
    }

    #[inline]
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    #[inline]
    fn depth_bias(&self) -> f32 {
        0.0
    }
}
