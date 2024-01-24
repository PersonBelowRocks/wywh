use bevy::{
    ecs::{
        query::ROQueryItem,
        system::{lifetimeless::SRes, Commands, Res, Resource, SystemParamItem},
        world::{FromWorld, World},
    },
    render::{
        extract_resource::ExtractResource,
        render_asset::RenderAssets,
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::{
            BindGroup, BindGroupEntry, BindGroupLayoutEntry, BindingResource, BindingType, Buffer,
            BufferBinding, BufferBindingType, BufferInitDescriptor, ShaderStages, StorageBuffer,
            TextureSampleType, TextureView, TextureViewDimension,
        },
        renderer::{RenderDevice, RenderQueue},
        texture::Image,
        Extract,
    },
};

use crate::data::{
    registries::texture::TexregFaces,
    systems::{VoxelColorTextureAtlas, VoxelNormalTextureAtlas},
    texture::GpuFaceTexture,
};

#[derive(Clone, Resource)]
pub struct RegistryBindGroup {
    pub bind_group: BindGroup,
}

// TODO: texture bindings
impl FromWorld for RegistryBindGroup {
    fn from_world(world: &mut World) -> Self {
        let gpu = world.resource::<RenderDevice>();
        let queue = world.resource::<RenderQueue>();

        let extracted_faces = world.resource::<ExtractedTexregFaces>();

        let mut buffer = StorageBuffer::<Vec<GpuFaceTexture>>::from(extracted_faces.faces.clone());
        buffer.set_label(Some("face_texture_buffer"));
        buffer.write_buffer(&gpu, &queue);

        let gpu_buffer = buffer.buffer().unwrap();

        let layout = gpu.create_bind_group_layout(
            Some("registry_bind_group_layout"),
            &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        );

        let bind_group = gpu.create_bind_group(
            Some("registry_bind_group"),
            &layout,
            &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &gpu_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        );

        RegistryBindGroup { bind_group }
    }
}

#[derive(Clone, Resource)]
pub struct ExtractedTexregFaces {
    pub faces: Vec<GpuFaceTexture>,
}

pub fn extract_texreg_faces(mut cmds: Commands, texreg_faces: Extract<Res<TexregFaces>>) {
    cmds.insert_resource(ExtractedTexregFaces {
        faces: texreg_faces.0.clone(),
    })
}

pub fn prepare_gpu_face_texture_buffer(
    mut cmds: Commands,
    extracted_faces: Option<Res<ExtractedTexregFaces>>,
) {
    // we can only initialize the registry bind group resource if the faces have been extracted
    if extracted_faces.is_some() {
        cmds.init_resource::<RegistryBindGroup>()
    }
}

pub struct SetRegistryBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetRegistryBindGroup<I> {
    type Param = SRes<RegistryBindGroup>;

    type ViewData = ();
    type ItemData = ();

    fn render<'w>(
        _item: &P,
        _view: ROQueryItem<'w, Self::ViewData>,
        _entity: ROQueryItem<'w, Self::ItemData>,
        param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let param = param.into_inner();
        pass.set_bind_group(I, &param.bind_group, &[]);
        RenderCommandResult::Success
    }
}
