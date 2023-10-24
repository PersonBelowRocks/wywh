use bevy::{
    pbr::{
        ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
        MaterialPipeline, MaterialPipelineKey,
    },
    prelude::{Asset, Material, StandardMaterial},
    reflect::TypePath,
    render::{
        mesh::{MeshVertexAttribute, MeshVertexBufferLayout},
        render_resource::{
            AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
            VertexFormat,
        },
    },
};

#[derive(AsBindGroup, Asset, TypePath, Clone, Debug)]
pub struct GreedyMeshMaterial {}

impl GreedyMeshMaterial {
    pub const TEXTURE_MESH_ATTR: MeshVertexAttribute =
        MeshVertexAttribute::new("Greedy_Texture", 4099_1, VertexFormat::Float32x2);
}

impl MaterialExtension for GreedyMeshMaterial {
    fn specialize(
        pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayout,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.label = Some("silly_pipeline".into());

        let buffers = layout.get_layout(&[Self::TEXTURE_MESH_ATTR.at_shader_location(10)])?;

        // descriptor.vertex.buffers.push(buffers);

        Ok(())
    }

    fn vertex_shader() -> ShaderRef {
        "shaders/greedy_mesh_material.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/greedy_mesh_material.wgsl".into()
    }
}
