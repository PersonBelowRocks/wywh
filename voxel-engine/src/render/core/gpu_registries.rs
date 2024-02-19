use bevy::{
    ecs::{
        query::ROQueryItem,
        system::{lifetimeless::SRes, Commands, Res, Resource, SystemParamItem},
    },
    log::info,
    render::{
        render_asset::RenderAssets,
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::{
            BindGroup, BindGroupEntries, BindingResource, BufferBinding, StorageBuffer,
        },
        renderer::{RenderDevice, RenderQueue},
        texture::Image,
        Extract,
    },
};
use mip_texture_array::asset::MippedArrayTexture;

use crate::data::{
    registries::texture::TexregFaces, systems::ArrayTextureHandles, texture::GpuFaceTexture,
};

use super::DefaultBindGroupLayouts;

#[derive(Clone, Resource)]
pub struct RegistryBindGroup {
    pub bind_group: BindGroup,
}

#[derive(Clone, Resource)]
pub struct ExtractedTexregFaces {
    pub faces: Vec<GpuFaceTexture>,
}

pub fn extract_texreg_faces(mut cmds: Commands, texreg_faces: Extract<Option<Res<TexregFaces>>>) {
    if let Some(texreg_faces) = texreg_faces.as_ref() {
        cmds.insert_resource(ExtractedTexregFaces {
            faces: texreg_faces.0.clone(),
        });
        info!("Extracted texture registry faces into render world.");
    }
}

pub fn prepare_gpu_registry_data(
    mut cmds: Commands,
    extracted_faces: Option<Res<ExtractedTexregFaces>>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    layouts: Res<DefaultBindGroupLayouts>,
    array_textures: Res<RenderAssets<MippedArrayTexture>>,
    handles: ArrayTextureHandles,
) {
    // we can only initialize the registry bind group resource if the faces and textures have been extracted
    let Some(extracted_faces) = extracted_faces else {
        return;
    };

    let Ok(gpu_array_textures) = handles.get_render_assets(array_textures.as_ref()) else {
        return;
    };

    let mut buffer = StorageBuffer::<Vec<GpuFaceTexture>>::from(extracted_faces.faces.clone());
    buffer.set_label(Some("face_texture_buffer"));
    buffer.write_buffer(&gpu, &queue);

    let gpu_buffer = buffer.buffer().unwrap();

    let bind_group = gpu.create_bind_group(
        Some("registry_bind_group"),
        &layouts.registry_bg_layout,
        &BindGroupEntries::sequential((
            BindingResource::Buffer(BufferBinding {
                buffer: &gpu_buffer,
                offset: 0,
                size: None,
            }),
            &gpu_array_textures.color.texture_view,
            &gpu_array_textures.color.sampler,
            &gpu_array_textures.normal.texture_view,
            &gpu_array_textures.normal.sampler,
        )),
    );

    info!("Queued texture registry data for the GPU");

    cmds.insert_resource(RegistryBindGroup { bind_group });
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
