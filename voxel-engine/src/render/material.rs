use bevy::{
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    reflect::{TypeUuid, TypePath},
    render::{
        mesh::MeshVertexBufferLayout,
        render_resource::{
            AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError, ShaderDefVal,
        },
    },
};

use super::mesh::ChunkMesh;

#[derive(AsBindGroup, Clone, TypeUuid, TypePath)]
#[uuid = "88243925-82d2-494c-9c72-c58a0a6378f1"]
pub struct VoxelChunkMaterial {}

macro_rules! uint_shader_def {
    ($label:ident) => {
        ShaderDefVal::UInt(stringify!($label).to_string(), $label)
    };
}

impl Material for VoxelChunkMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/voxel_chunk_mat.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/voxel_chunk_mat.wgsl".into()
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayout,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        use super::vertex::consts::*;

        let shader_defs = vec![
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

        let vertex_layout = layout.get_layout(&[
            ChunkMesh::VOXEL_DATA_ATTR.at_shader_location(0),
        ])?;

        descriptor.vertex.buffers = vec![vertex_layout];
        descriptor.vertex.shader_defs.extend_from_slice(&shader_defs);
        descriptor.fragment.as_mut().unwrap().shader_defs.extend_from_slice(&shader_defs);

        Ok(())
    }
}
