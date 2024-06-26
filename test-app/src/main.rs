extern crate voxel_engine as ve;

mod camera;
mod debug_info;

use std::env;
use std::f32::consts::PI;

use bevy::core_pipeline::experimental::taa::{TemporalAntiAliasBundle, TemporalAntiAliasPlugin};
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;

use bevy::log::{self, LogPlugin};
use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::pbr::ScreenSpaceAmbientOcclusionBundle;
use bevy::prelude::*;

use bevy::render::settings::{WgpuFeatures, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::window::PresentMode;
use debug_info::{DirectionText, FpsText, SpatialDebugText};
use ve::topo::ChunkObserver;
use ve::EngineState;

fn main() {
    println!(
        "RUNNING IN WORKING DIRECTORY: {}",
        env::current_dir().unwrap().to_string_lossy()
    );

    App::new()
        .insert_resource(ClearColor(Color::rgb(0.4, 0.75, 0.9)))
        .add_plugins((
            DefaultPlugins
                .set(RenderPlugin {
                    render_creation: WgpuSettings {
                        features: WgpuFeatures::POLYGON_MODE_LINE,
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
                    filter: "info".into(),
                    level: log::Level::DEBUG,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        present_mode: PresentMode::Immediate,
                        ..default()
                    }),
                    ..default()
                }),
            WireframePlugin,
            TemporalAntiAliasPlugin,
            ve::VoxelPlugin::new(vec!["test-app\\assets\\variants".into()]),
            FrameTimeDiagnosticsPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                camera::kb_controls,
                camera::mouse_controls,
                camera::cursor_grab,
            ),
        )
        .add_systems(
            Update,
            (
                debug_info::update_spatial_debug_text.run_if(in_state(EngineState::Finished)),
                debug_info::chunk_borders,
                debug_info::update_direction_text,
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
                top: Val::Percent(2.0),
                right: Val::Percent(2.0),
                flex_direction: FlexDirection::Row,
                ..default()
            }),
        SpatialDebugText,
    ));

    commands.spawn((
        TextBundle::from_section(
            "facing",
            TextStyle {
                color: Color::WHITE,
                font_size: 35.0,
                ..default()
            },
        )
        .with_text_justify(JustifyText::Left)
        .with_style(Style {
            position_type: PositionType::Absolute,
            bottom: Val::Percent(85.0),
            right: Val::Percent(90.0),
            ..default()
        }),
        DirectionText,
    ));

    commands.spawn((
        TextBundle::from_sections([
            TextSection {
                value: "FPS: ".into(),
                style: TextStyle {
                    color: Color::WHITE,
                    font_size: 35.0,
                    ..default()
                },
            },
            TextSection {
                value: "N/A".into(),
                style: TextStyle {
                    color: Color::WHITE,
                    font_size: 35.0,
                    ..default()
                },
            },
        ])
        .with_text_justify(JustifyText::Left)
        .with_style(Style {
            position_type: PositionType::Absolute,
            bottom: Val::Percent(80.0),
            right: Val::Percent(90.0),
            ..default()
        }),
        FpsText,
    ));

    commands.spawn(PbrBundle {
        mesh: meshes.add(Rectangle::from_size(Vec2::splat(2.0))),
        material: materials.add(Color::rgb(0.3, 0.5, 0.3)),
        ..default()
    });
    // cube
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::from_size(Vec3::ONE)),
        material: materials.add(Color::rgb(0.8, 0.7, 0.6)),
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
    commands
        .spawn((
            Camera3dBundle {
                transform: Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
                projection: Projection::Perspective(PerspectiveProjection {
                    fov: 100.0 * (PI / 180.0),
                    ..default()
                }),
                ..default()
            },
            camera::PlayerCamController::default(),
            ChunkObserver {
                horizontal_range: 5.0,
                view_distance_above: 3.0,
                view_distance_below: 3.0,
            },
            VisibilityBundle::default(),
            ScreenSpaceAmbientOcclusionBundle::default(),
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
                camera::PlayerHeadlight,
            ));
        });
}
