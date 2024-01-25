use bevy::{
    asset::{AssetServer, Handle},
    core_pipeline::{
        core_3d::{Camera3d, Opaque3d},
        prepass::{DeferredPrepass, DepthPrepass, MotionVectorPrepass, NormalPrepass},
        tonemapping::{DebandDither, Tonemapping},
    },
    ecs::{
        query::Has,
        system::{Query, Res, ResMut, Resource},
        world::{FromWorld, World},
    },
    log::error,
    pbr::{
        DrawMesh, MeshPipeline, MeshPipelineKey, RenderMeshInstances,
        ScreenSpaceAmbientOcclusionSettings, SetMeshBindGroup, SetMeshViewBindGroup,
        ShadowFilteringMethod,
    },
    render::{
        camera::{Projection, TemporalJitter},
        mesh::{Mesh, MeshVertexBufferLayout},
        render_asset::RenderAssets,
        render_phase::{DrawFunctions, RenderPhase, SetItemPipeline},
        render_resource::{
            binding_types::{storage_buffer, storage_buffer_read_only},
            BindGroupLayout, BindGroupLayoutEntries, PipelineCache, RenderPipelineDescriptor,
            Shader, ShaderStages, SpecializedMeshPipeline, SpecializedMeshPipelineError,
            SpecializedMeshPipelines,
        },
        renderer::RenderDevice,
        view::{ExtractedView, VisibleEntities},
    },
};

use crate::{data::texture::GpuFaceTexture, render::quad::GpuQuad};

use super::{
    gpu_chunk::{ChunkRenderData, ChunkRenderDataStore, SetChunkBindGroup},
    gpu_registries::SetRegistryBindGroup,
};

#[derive(Resource, Clone)]
pub struct VoxelChunkPipeline {
    pub mesh_pipeline: MeshPipeline,
    pub registry_layout: BindGroupLayout,
    pub chunk_layout: BindGroupLayout,
    pub vert: Handle<Shader>,
    pub frag: Handle<Shader>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoxelChunkPipelineKey {
    pub mesh_key: MeshPipelineKey,
}

impl FromWorld for VoxelChunkPipeline {
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>();
        let gpu = world.resource::<RenderDevice>();

        let registry_layout = gpu.create_bind_group_layout(
            Some("registry_bind_group_layout"),
            &BindGroupLayoutEntries::with_indices(
                ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ((0, storage_buffer_read_only::<GpuFaceTexture>(false)),),
            ),
        );

        let chunk_layout = gpu.create_bind_group_layout(
            Some("registry_bind_group_layout"),
            &BindGroupLayoutEntries::with_indices(
                ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                (
                    (0, storage_buffer_read_only::<GpuQuad>(false)),
                    (1, storage_buffer_read_only::<u32>(false)),
                ),
            ),
        );

        Self {
            mesh_pipeline: world.resource::<MeshPipeline>().clone(),
            registry_layout,
            chunk_layout,
            vert: server.load("shaders/greedy_mesh_vert.wgsl"),
            frag: server.load("shaders/greedy_mesh_frag.wgsl"),
        }
    }
}

impl SpecializedMeshPipeline for VoxelChunkPipeline {
    type Key = VoxelChunkPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayout,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut descriptor = self.mesh_pipeline.specialize(key.mesh_key, layout)?;

        // TODO: add bind group layouts to the descriptor

        descriptor.vertex.shader = self.vert.clone();
        descriptor.fragment.as_mut().unwrap().shader = self.frag.clone();

        Ok(descriptor)
    }
}

pub const fn tonemapping_pipeline_key(tonemapping: Tonemapping) -> MeshPipelineKey {
    match tonemapping {
        Tonemapping::None => MeshPipelineKey::TONEMAP_METHOD_NONE,
        Tonemapping::Reinhard => MeshPipelineKey::TONEMAP_METHOD_REINHARD,
        Tonemapping::ReinhardLuminance => MeshPipelineKey::TONEMAP_METHOD_REINHARD_LUMINANCE,
        Tonemapping::AcesFitted => MeshPipelineKey::TONEMAP_METHOD_ACES_FITTED,
        Tonemapping::AgX => MeshPipelineKey::TONEMAP_METHOD_AGX,
        Tonemapping::SomewhatBoringDisplayTransform => {
            MeshPipelineKey::TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM
        }
        Tonemapping::TonyMcMapface => MeshPipelineKey::TONEMAP_METHOD_TONY_MC_MAPFACE,
        Tonemapping::BlenderFilmic => MeshPipelineKey::TONEMAP_METHOD_BLENDER_FILMIC,
    }
}

pub fn queue_chunks(
    functions: Res<DrawFunctions<Opaque3d>>,
    pipeline: Res<VoxelChunkPipeline>,
    mut pipelines: ResMut<SpecializedMeshPipelines<VoxelChunkPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    mut render_mesh_instances: ResMut<RenderMeshInstances>,
    render_meshes: Res<RenderAssets<Mesh>>,
    chunk_data_store: Res<ChunkRenderDataStore>,
    mut views: Query<(
        &ExtractedView,
        &VisibleEntities,
        &mut RenderPhase<Opaque3d>,
        Option<&Tonemapping>,
        Option<&DebandDither>,
        Option<&ShadowFilteringMethod>,
        Option<&Projection>,
        (
            Has<NormalPrepass>,
            Has<DepthPrepass>,
            Has<MotionVectorPrepass>,
            Has<DeferredPrepass>,
        ),
        Has<ScreenSpaceAmbientOcclusionSettings>,
        Has<TemporalJitter>,
    )>,
) {
    let draw_chunk = functions.read().id::<DrawVoxelChunk>();

    for (
        view,
        visible_entities,
        mut phase,
        tonemapping,
        dither,
        shadow_filter_method,
        projection,
        (normal_prepass, depth_prepass, motion_vector_prepass, deferred_prepass),
        ssao,
        temporal_jitter,
    ) in views.iter_mut()
    {
        let mut view_key = MeshPipelineKey::from_hdr(view.hdr);

        if normal_prepass {
            view_key |= MeshPipelineKey::NORMAL_PREPASS;
        }

        if depth_prepass {
            view_key |= MeshPipelineKey::DEPTH_PREPASS;
        }

        if motion_vector_prepass {
            view_key |= MeshPipelineKey::MOTION_VECTOR_PREPASS;
        }

        if deferred_prepass {
            view_key |= MeshPipelineKey::DEFERRED_PREPASS;
        }

        if temporal_jitter {
            view_key |= MeshPipelineKey::TEMPORAL_JITTER;
        }

        if ssao {
            view_key |= MeshPipelineKey::SCREEN_SPACE_AMBIENT_OCCLUSION;
        }

        if let Some(projection) = projection {
            view_key |= match projection {
                Projection::Perspective(_) => MeshPipelineKey::VIEW_PROJECTION_PERSPECTIVE,
                Projection::Orthographic(_) => MeshPipelineKey::VIEW_PROJECTION_ORTHOGRAPHIC,
            };
        }

        match shadow_filter_method.unwrap_or(&ShadowFilteringMethod::default()) {
            ShadowFilteringMethod::Hardware2x2 => {
                view_key |= MeshPipelineKey::SHADOW_FILTER_METHOD_HARDWARE_2X2;
            }
            ShadowFilteringMethod::Castano13 => {
                view_key |= MeshPipelineKey::SHADOW_FILTER_METHOD_CASTANO_13;
            }
            ShadowFilteringMethod::Jimenez14 => {
                view_key |= MeshPipelineKey::SHADOW_FILTER_METHOD_JIMENEZ_14;
            }
        }

        if !view.hdr {
            if let Some(tonemapping) = tonemapping {
                view_key |= MeshPipelineKey::TONEMAP_IN_SHADER;
                view_key |= tonemapping_pipeline_key(*tonemapping);
            }
            if let Some(DebandDither::Enabled) = dither {
                view_key |= MeshPipelineKey::DEBAND_DITHER;
            }
        }

        let rangefinder = view.rangefinder3d();
        for entity in &visible_entities.entities {
            // skip all entities that dont have chunk render data
            if !chunk_data_store
                .map
                .get(entity)
                .is_some_and(|data| matches!(data, ChunkRenderData::BindGroup(_)))
            {
                continue;
            }

            let Some(mesh_instance) = render_mesh_instances.get_mut(entity) else {
                continue;
            };
            let Some(mesh) = render_meshes.get(mesh_instance.mesh_asset_id) else {
                continue;
            };

            let mut mesh_key = view_key;

            mesh_key |= MeshPipelineKey::from_primitive_topology(mesh.primitive_topology);

            let pipeline_id = match pipelines.specialize(
                pipeline_cache.as_ref(),
                pipeline.as_ref(),
                VoxelChunkPipelineKey { mesh_key },
                &mesh.layout,
            ) {
                Ok(id) => id,
                Err(err) => {
                    error!("Error during voxel chunk pipeline specialization: {err}");
                    continue;
                }
            };

            let distance =
                rangefinder.distance_translation(&mesh_instance.transforms.transform.translation);

            // queue this entity for rendering
            phase.add(Opaque3d {
                entity: *entity,
                draw_function: draw_chunk,
                pipeline: pipeline_id,
                distance,
                batch_range: 0..1,
                dynamic_offset: None,
            });
        }
    }
}

pub type DrawVoxelChunk = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshBindGroup<1>,
    SetRegistryBindGroup<2>,
    SetChunkBindGroup<3>,
    DrawMesh,
);
