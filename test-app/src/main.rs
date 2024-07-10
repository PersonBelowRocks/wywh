extern crate voxel_engine as ve;

mod controls;
mod debug_info;

use std::env;
use std::f32::consts::PI;
use std::sync::Arc;

use bevy::core_pipeline::experimental::taa::{TemporalAntiAliasBundle, TemporalAntiAliasPlugin};
use bevy::core_pipeline::prepass::DeferredPrepass;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;

use bevy::ecs::entity::EntityHashSet;
use bevy::log::{self, LogPlugin};
use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::pbr::ScreenSpaceAmbientOcclusionBundle;
use bevy::prelude::*;

use bevy::render::settings::{WgpuFeatures, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy_renderdoc::RenderDocPlugin;
use crossbeam::channel::{self, Sender};
use debug_info::{DebugText, DirectionText, FpsText};
use ve::render::core::RenderCoreDebug;
use ve::render::lod::LevelOfDetail;
use ve::topo::controller::{BatchFlags, ChunkBatch, ChunkBatchLod, ObserverBundle, VisibleBatches};
use ve::topo::world::ChunkPos;
use ve::{CoreEngineSetup, EngineState};

#[derive(Resource)]
pub struct RenderCoreDebugSender {
    pub clear_inspections: Sender<()>,
    pub inspect: Sender<ChunkPos>,
}

fn main() {
    println!(
        "RUNNING IN WORKING DIRECTORY: {}",
        env::current_dir().unwrap().to_string_lossy()
    );

    let (ci_tx, ci_rx) = channel::unbounded::<()>();
    let (insp_tx, insp_rx) = channel::unbounded::<ChunkPos>();

    App::new()
        .insert_resource(RenderCoreDebugSender {
            clear_inspections: ci_tx,
            inspect: insp_tx,
        })
        .insert_resource(ClearColor(Color::srgb(0.4, 0.75, 0.9)))
        .add_plugins((
            RenderDocPlugin,
            DefaultPlugins
                .set(RenderPlugin {
                    render_creation: WgpuSettings {
                        features: WgpuFeatures::POLYGON_MODE_LINE
                            | WgpuFeatures::INDIRECT_FIRST_INSTANCE,
                        ..default()
                    }
                    .into(),
                    synchronous_pipeline_compilation: true,
                })
                .set(AssetPlugin {
                    mode: AssetMode::Unprocessed,
                    ..default()
                })
                .set(LogPlugin {
                    filter: "info,voxel_engine=debug".into(),
                    level: log::Level::DEBUG,
                    ..default()
                }),
            WireframePlugin,
            TemporalAntiAliasPlugin,
            ve::VoxelPlugin {
                variant_folders: Arc::new(vec!["test-app/assets/variants".into()]),
                render_core_debug: Some(RenderCoreDebug {
                    clear_inpsection: ci_rx,
                    inspect_chunks: insp_rx,
                }),
            },
            FrameTimeDiagnosticsPlugin,
        ))
        .add_systems(Startup, setup.after(CoreEngineSetup))
        .add_systems(
            Update,
            (
                controls::inspect,
                controls::kb_controls,
                controls::mouse_controls,
                controls::cursor_grab,
            ),
        )
        .add_systems(
            Update,
            (
                debug_info::update_debug_text.run_if(in_state(EngineState::Finished)),
                debug_info::chunk_borders,
                debug_info::fps_text_update_system,
            ),
        )
        .run();
}

fn setup(
    _wireframe_config: ResMut<WireframeConfig>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // wireframe_config.global = true;

    debug!("Setting up test-app");

    commands.spawn((
        TextBundle::default()
            .with_text_justify(JustifyText::Left)
            .with_style(Style {
                position_type: PositionType::Absolute,
                bottom: Val::Percent(2.0),
                left: Val::Percent(2.0),
                flex_direction: FlexDirection::Row,
                ..default()
            }),
        DebugText,
    ));

    commands.spawn((
        TextBundle::default()
            .with_text_justify(JustifyText::Left)
            .with_style(Style {
                position_type: PositionType::Absolute,
                top: Val::Percent(2.0),
                left: Val::Percent(2.0),
                flex_direction: FlexDirection::Row,
                ..default()
            }),
        FpsText,
    ));

    commands.spawn(PbrBundle {
        mesh: meshes.add(Rectangle::from_size(Vec2::splat(2.0))),
        material: materials.add(Color::srgb(0.3, 0.5, 0.3)),
        ..default()
    });
    // cube
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::from_size(Vec3::ONE)),
        material: materials.add(Color::srgb(0.8, 0.7, 0.6)),
        transform: Transform::from_xyz(-1.0, 0.5, -1.0),
        ..default()
    });

    // light
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            color: Color::WHITE,
            illuminance: 10000.0,
            shadows_enabled: true,

            ..default()
        },
        transform: Transform::from_rotation(Quat::from_euler(
            EulerRot::ZYX,
            0.0,
            PI * -0.15,
            PI * -0.15,
        )),
        ..default()
    });

    commands.insert_resource(Msaa::Off);
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 200.0,
    });

    // camera
    let observer_entity = commands
        .spawn((
            Camera3dBundle {
                transform: Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
                projection: Projection::Perspective(PerspectiveProjection {
                    fov: 100.0 * (PI / 180.0),
                    ..default()
                }),
                ..default()
            },
            controls::PlayerCamController::default(),
            ObserverBundle::new(),
            VisibilityBundle::default(),
            ScreenSpaceAmbientOcclusionBundle::default(),
            DeferredPrepass,
        ))
        .insert(TemporalAntiAliasBundle { ..default() })
        .with_children(|builder| {
            builder.spawn((
                SpotLightBundle {
                    spot_light: SpotLight {
                        color: Color::WHITE,
                        intensity: 10000000.0,
                        shadows_enabled: true,
                        inner_angle: PI / 8.0 * 0.85,
                        outer_angle: PI / 8.0,
                        range: 10000.0,

                        ..default()
                    },

                    ..default()
                },
                controls::PlayerHeadlight,
            ));
        })
        .id();

    let batch_entity = commands
        .spawn((
            ChunkBatch::new(observer_entity, BatchFlags::RENDER),
            ChunkBatchLod(LevelOfDetail::X16Subdiv),
        ))
        .id();

    commands
        .get_or_spawn(observer_entity)
        .insert(VisibleBatches(EntityHashSet::from_iter([batch_entity])));
}
