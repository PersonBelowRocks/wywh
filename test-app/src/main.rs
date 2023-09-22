extern crate voxel_engine as ve;

mod camera;
mod debug_info;

use std::f32::consts::PI;

use bevy::pbr::{CascadeShadowConfigBuilder, ScreenSpaceAmbientOcclusionBundle};
use bevy::prelude::*;
use debug_info::{DirectionText, PositionText};
use ve::data::tile::VoxelId;
use ve::topo::chunk::{Chunk, ChunkPos};
use ve::topo::generator::{GenerateChunk, GeneratorChoice};
use ve::ChunkEntity;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(ve::VoxelPlugin)
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
                debug_info::chunk_borders,
                debug_info::update_direction_text,
            ),
        )
        .run();
}

fn setup(
    mut writer: EventWriter<GenerateChunk<VoxelId>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for x in -12..12 {
        for y in -12..12 {
            for z in -12..12 {
                writer.send(GenerateChunk {
                    pos: IVec3::new(x, y, z).into(),
                    generator: GeneratorChoice::Default,
                    default_value: VoxelId::new(0),
                })
            }
        }
    }

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
        .with_text_alignment(TextAlignment::Left)
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
        .with_text_alignment(TextAlignment::Left)
        .with_style(Style {
            position_type: PositionType::Absolute,
            bottom: Val::Percent(85.0),
            right: Val::Percent(90.0),
            ..default()
        }),
        DirectionText,
    ));

    commands.spawn(PbrBundle {
        mesh: meshes.add(shape::Plane::from_size(5.0).into()),
        material: materials.add(Color::rgb(0.3, 0.5, 0.3).into()),
        ..default()
    });
    // cube
    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
        material: materials.add(Color::rgb(0.8, 0.7, 0.6).into()),
        transform: Transform::from_xyz(-1.0, 0.5, -1.0),
        ..default()
    });

    /*
    // light
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            color: Color::WHITE,
            illuminance: 100000.0,
            shadows_enabled: true,

            // TODO: this should be done in the shader for voxel chunks!
            // we need to increase the shadow depth bias in order to avoid shadow acne on voxels.
            .. default()
        },
        transform: Transform::from_rotation(Quat::from_euler(
            EulerRot::ZYX,
            0.0,
            PI * -0.15,
            PI * -0.15,
        )),
        ..default()
    });
    */

    commands.insert_resource(Msaa::Off);

    // camera
    commands
        .spawn(Camera3dBundle {
            transform: Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        })
        .insert(camera::PlayerCamController::default())
        .insert(VisibilityBundle::default())
        // .insert(ScreenSpaceAmbientOcclusionBundle::default())
        .with_children(|builder| {
            builder.spawn((
                SpotLightBundle {
                    spot_light: SpotLight {
                        color: Color::WHITE,
                        intensity: 300000.0,
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
