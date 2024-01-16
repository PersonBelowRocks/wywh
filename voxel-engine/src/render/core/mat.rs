use bevy::{
    asset::Asset,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    reflect::TypePath,
    render::{
        mesh::{Mesh, MeshVertexAttribute, MeshVertexBufferLayout},
        render_asset::RenderAssets,
        render_resource::{
            AsBindGroup, AsBindGroupError, BindGroupLayout, BindGroupLayoutEntry, BindingType,
            Buffer, BufferBindingType, BufferInitDescriptor, BufferUsages, OwnedBindingResource,
            RenderPipelineDescriptor, ShaderDefVal, ShaderRef, ShaderStages,
            SpecializedMeshPipelineError, UnpreparedBindGroup, VertexFormat,
        },
        renderer::RenderDevice,
        texture::{FallbackImage, Image},
    },
};

use crate::data::texture::GpuFaceTexture;

#[derive(Debug, Clone, Asset, TypePath)]
pub struct VxlChunkMaterial {
    faces: Buffer,
    face_count: usize,

    occlusion: Vec<u8>,
    occlusion_count: usize,
}

impl AsBindGroup for VxlChunkMaterial {
    type Data = ();

    fn unprepared_bind_group(
        &self,
        layout: &BindGroupLayout,
        gpu: &RenderDevice,
        images: &RenderAssets<Image>,
        fallback_image: &FallbackImage,
    ) -> Result<UnpreparedBindGroup<Self::Data>, AsBindGroupError> {
        let occlusion_buffer = gpu.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("occlusion_buffer"),
            contents: &self.occlusion[..],
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        });

        let face_buffer = self.faces.clone();

        let bg = UnpreparedBindGroup {
            data: (),
            bindings: vec![
                (100, OwnedBindingResource::Buffer(face_buffer)),
                (101, OwnedBindingResource::Buffer(occlusion_buffer)),
            ],
        };

        Ok(bg)
    }

    fn bind_group_layout_entries(gpu: &RenderDevice) -> Vec<BindGroupLayoutEntry>
    where
        Self: Sized,
    {
        vec![
            BindGroupLayoutEntry {
                binding: 100,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 101,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ]
    }
}

impl VxlChunkMaterial {
    pub const TEXTURE_MESH_ATTR: MeshVertexAttribute =
        MeshVertexAttribute::new("Face_Index", 4099_1, VertexFormat::Uint32);

    pub const BITFIELDS_ATTR: MeshVertexAttribute =
        MeshVertexAttribute::new("Bitfields", 4099_2, VertexFormat::Uint32);
}

macro_rules! uint_shader_def {
    ($label:ident) => {
        ShaderDefVal::UInt(stringify!($label).to_string(), $label)
    };
}

impl MaterialExtension for VxlChunkMaterial {
    fn specialize(
        pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayout,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        use crate::render::quad::consts::*;

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
            Self::BITFIELDS_ATTR.at_shader_location(11),
        ])?;

        descriptor.vertex.buffers = vec![buffer];

        descriptor
            .vertex
            .shader_defs
            .extend_from_slice(&shader_defs);

        let Some(fragment) = descriptor.fragment.as_mut() else {
            panic!("Pipeline descriptor doesn't have fragment state");
        };

        fragment.shader_defs.extend_from_slice(&shader_defs);
        fragment
            .shader_defs
            .extend_from_slice(&["VERTEX_TANGENTS".into()]);

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
