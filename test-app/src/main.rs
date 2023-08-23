extern crate voxel_engine as ve;

mod camera;
mod debug_info;

use std::f32::consts::PI;

use bevy::pbr::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use debug_info::PositionText;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(ve::VoxelPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (camera::kb_controls, camera::mouse_controls))
        .add_systems(Update, debug_info::update_position_text)
        .run();
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    
    commands.spawn((
        TextBundle::from_sections([
            TextSection::new(
                "x",
                TextStyle {
                    font_size: 35.0,
                    color: Color::WHITE,
                    .. default()
                }
            ),
            TextSection::new(
                "y",
                TextStyle {
                    font_size: 35.0,
                    color: Color::WHITE,
                    .. default()
                }
            ),
            TextSection::new(
                "z",
                TextStyle {
                    font_size: 35.0,
                    color: Color::WHITE,
                    .. default()
                }
            )
        ])
        .with_text_alignment(TextAlignment::Left)
        .with_style(Style {
            position_type: PositionType::Absolute,
            bottom: Val::Percent(85.0),
            right: Val::Percent(10.0),
            .. default()
        }),
        PositionText,
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
    
    // light
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            color: Color::WHITE,
            illuminance: 100000.0,
            shadows_enabled: true,
            
            // TODO: this should be done in the shader for voxel chunks!
            // we need to increase the shadow depth bias in order to avoid shadow acne on voxels.
            shadow_depth_bias: 0.075,

            shadow_normal_bias: 0.6
        },
        transform: Transform::from_rotation(Quat::from_euler(
            EulerRot::ZYX,
            0.0,
            PI * -0.15,
            PI * -0.15,
        )),
        ..default()
    });
    
    // camera
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    }).insert(camera::PlayerCamera);
}
