extern crate voxel_engine as ve;

mod camera;
mod debug_info;

use std::f32::consts::PI;

use bevy::core_pipeline::experimental::taa::{TemporalAntiAliasBundle, TemporalAntiAliasPlugin};
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;

use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::pbr::ScreenSpaceAmbientOcclusionBundle;
use bevy::prelude::*;

use bevy::render::settings::{WgpuFeatures, WgpuSettings};
use bevy::render::RenderPlugin;
use debug_info::{DirectionText, FpsText, PositionText};

use ve::topo::generator::{GenerateChunk, GeneratorChoice};

fn main() {
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
                debug_info::update_position_text,
                // debug_info::chunk_borders,
                debug_info::update_direction_text,
                debug_info::fps_text_update_system,
            ),
        )
        .run();
}

fn setup(
    mut writer: EventWriter<GenerateChunk>,
    _wireframe_config: ResMut<WireframeConfig>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                writer.send(GenerateChunk {
                    pos: IVec3::new(x, y, z).into(),
                    generator: GeneratorChoice::Default,
                });
            }
        }
    }

    // wireframe_config.global = true;

    commands.spawn((
        TextBundle::from_sections([
            TextSection::new(
                "x",
                TextStyle {
                    font_size: 35.0,
                    color: Color::WHITE,
                    ..default()
                },
            ),
            TextSection::new(
                "y",
                TextStyle {
                    font_size: 35.0,
                    color: Color::WHITE,
                    ..default()
                },
            ),
            TextSection::new(
                "z",
                TextStyle {
                    font_size: 35.0,
                    color: Color::WHITE,
                    ..default()
                },
            ),
        ])
        .with_text_justify(JustifyText::Left)
        .with_style(Style {
            position_type: PositionType::Absolute,
            bottom: Val::Percent(85.0),
            right: Val::Percent(10.0),
            ..default()
        }),
        PositionText,
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
            illuminance: 100000.0,
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
        brightness: 0.3,
    });

    // camera
    commands
        .spawn((
            Camera3dBundle {
                transform: Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
                ..default()
            },
            camera::PlayerCamController::default(),
            VisibilityBundle::default(),
            ScreenSpaceAmbientOcclusionBundle::default(),
        ))
        .insert(TemporalAntiAliasBundle { ..default() })
        .with_children(|builder| {
            builder.spawn((
                SpotLightBundle {
                    spot_light: SpotLight {
                        color: Color::WHITE,
                        intensity: 3000.0,
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
