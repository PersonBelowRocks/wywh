use bevy::{
    pbr::{MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    prelude::{info, Asset, Mesh},
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
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayout,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // descriptor.label = Some("silly_pipeline".into());

        let buffer = layout.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            Self::TEXTURE_MESH_ATTR.at_shader_location(10),
        ])?;

        descriptor.vertex.buffers = vec![buffer];
        info!("{:?}", descriptor.vertex.buffers);

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
