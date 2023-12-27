use bevy::{
    log::info,
    math::Vec2,
    pbr::{MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    prelude::{debug, Asset, Mesh},
    reflect::TypePath,
    render::{
        mesh::{MeshVertexAttribute, MeshVertexBufferLayout},
        render_resource::{
            AsBindGroup, RenderPipelineDescriptor, ShaderDefVal, ShaderRef,
            SpecializedMeshPipelineError, VertexFormat,
        },
    },
};

use crate::data::texture::GpuFaceTexture;

#[derive(AsBindGroup, Asset, TypePath, Clone, Debug)]
pub struct GreedyMeshMaterial {
    #[uniform(100)]
    pub texture_scale: f32,
    #[storage(101, read_only)]
    pub faces: Vec<GpuFaceTexture>,
}

impl GreedyMeshMaterial {
    pub const TEXTURE_MESH_ATTR: MeshVertexAttribute =
        MeshVertexAttribute::new("Greedy_Texture", 4099_1, VertexFormat::Uint32);

    pub const MISC_DATA_ATTR: MeshVertexAttribute =
        MeshVertexAttribute::new("Misc_Data", 4099_2, VertexFormat::Uint32);
}

macro_rules! uint_shader_def {
    ($label:ident) => {
        ShaderDefVal::UInt(stringify!($label).to_string(), $label)
    };
}

impl MaterialExtension for GreedyMeshMaterial {
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayout,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        use crate::render::quad::consts::*;

        info!("Specializing pipeline '{:?}'", descriptor.label);

        let shader_defs = [
            uint_shader_def!(ROTATION_MASK),
            uint_shader_def!(FLIP_UV_X),
            uint_shader_def!(FLIP_UV_Y),
            uint_shader_def!(OCCLUSION),
            ShaderDefVal::UInt(
                "HAS_NORMAL_MAP_BIT".into(),
                GpuFaceTexture::HAS_NORMAL_MAP_BIT,
            ),
            "VERTEX_UVS".into(),
        ];

        let buffer = layout.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Self::TEXTURE_MESH_ATTR.at_shader_location(10),
            Self::MISC_DATA_ATTR.at_shader_location(11),
        ])?;

        descriptor.vertex.buffers = vec![buffer];
        debug!("{:?}", descriptor.vertex.buffers);

        descriptor
            .vertex
            .shader_defs
            .extend_from_slice(&shader_defs);
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.extend_from_slice(&shader_defs);
            fragment
                .shader_defs
                .extend_from_slice(&["VERTEX_TANGENTS".into()])
        }

        Ok(())
    }

    fn vertex_shader() -> ShaderRef {
        "shaders/greedy_mesh_vert.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/greedy_mesh_frag.wgsl".into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        "shaders/greedy_mesh_prepass.wgsl".into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        "shaders/greedy_mesh_prepass.wgsl".into()
    }
}
