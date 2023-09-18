use bevy::prelude::*;
use voxel_engine::{data::tile::Face, topo::chunk::Chunk, ChunkEntity};

use crate::camera::PlayerCamController;

#[derive(Component)]
pub struct PositionText;

#[derive(Component)]
pub struct DirectionText;

pub fn update_position_text(
    mut q: Query<&mut Text, With<PositionText>>,
    player_q: Query<&Transform, With<PlayerCamController>>,
) {
    let pos = player_q.single().translation;

    for mut text in &mut q {
        text.sections[0].value = format!("x: {:.5}\n", pos.x);
        text.sections[1].value = format!("y: {:.5}\n", pos.y);
        text.sections[2].value = format!("z: {:.5}\n", pos.z);
    }
}

pub fn update_direction_text(
    mut q: Query<&mut Text, With<DirectionText>>,
    player_q: Query<&Transform, With<PlayerCamController>>,
) {
    let tfm = player_q.single();
    let mut text = q.single_mut();

    let cardinal: Face = {
        let fwd_xz = tfm.forward().xz();

        if fwd_xz.x.abs() > fwd_xz.y.abs() {
            if fwd_xz.x >= 0.0 {
                Face::North
            } else {
                Face::South
            }
        } else if fwd_xz.y >= 0.0 {
            Face::East
        } else {
            Face::West
        }
    };

    let direction_letter = match cardinal {
        Face::North => "N",
        Face::East => "E",
        Face::South => "S",
        Face::West => "W",

        _ => panic!("Unexpected cardinal direction"),
    };

    text.sections[0].value = format!("Facing: {0}", direction_letter)
}

pub fn chunk_borders(mut giz: Gizmos, chunks: Query<(&Transform, With<ChunkEntity>)>) {
    for (chunk_tf, _) in chunks.iter() {
        let gizmo_translation = chunk_tf.translation + (Vec3::splat(Chunk::SIZE as _) / 2.0);

        let gizmo_tf = Transform::from_translation(gizmo_translation)
            .with_scale(Vec3::splat(Chunk::SIZE as _));
        giz.cuboid(gizmo_tf, Color::LIME_GREEN);
    }
}
