use bevy::prelude::*;

use crate::camera::PlayerCamera;


#[derive(Component)]
pub struct PositionText;

pub fn update_position_text(mut q: Query<&mut Text, With<PositionText>>, player_q: Query<&Transform, With<PlayerCamera>>) {
    let pos = player_q.single().translation;

    for mut text in &mut q {
        text.sections[0].value = format!("x: {:.5}\n", pos.x);
        text.sections[1].value = format!("y: {:.5}\n", pos.y);
        text.sections[2].value = format!("z: {:.5}\n", pos.z);
    }
}