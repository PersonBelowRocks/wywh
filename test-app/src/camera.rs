use std::f32::consts::FRAC_PI_2;
use std::f32::consts::PI;

use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;

#[derive(Component, Default, Copy, Clone)]
pub struct PlayerCamera;




// pub fn mouse_controls(mut events: EventReader<MouseMotion>, mut q: Query<&mut Transform, With<PlayerCamera>>) {
//     const SENSITIVITY: f32 = 0.05;

//     let mut tfm = q.single_mut();

//     let (pitch, yaw, _) = tfm.rotation.to_euler(EulerRot::XYZ);

//     let mut mouse_y = 0.0;
//     let mut mouse_x = 0.0;

//     for mouse in events.iter() {
//         mouse_y += mouse.delta.y;
//         mouse_x += mouse.delta.x;
//     }

//     let new_pitch = (pitch + (mouse_y * PI/180.0)).clamp(-FRAC_PI_2, FRAC_PI_2);
//     let new_yaw = yaw + (mouse_x * PI/180.0);

//     tfm.rotation = Quat::from_euler(EulerRot::XYZ, new_pitch, new_yaw, 0.0);
// }

type Rotation = (f32, f32);

pub(crate) fn mouse_controls(mut rot: Local<Rotation>, mut events: EventReader<MouseMotion>, mut player: Query<&mut Transform, With<PlayerCamera>>) {
    const SENSITIVITY: f32 = 0.05;
    
    let mut pitch: f32 = 0.0;
    let mut yaw: f32 = 0.0;

    for mouse in events.iter() {
        pitch += mouse.delta.y * SENSITIVITY;
        yaw += -mouse.delta.x * SENSITIVITY;
    }

    let new_pitch = rot.0 + (pitch * PI/180.0);
    if (-PI/2.0..=PI/2.0).contains(&new_pitch) {
        rot.0 = new_pitch;
    }
    rot.1 += yaw * PI/180.0;


    let mut trans = player.single_mut();
    trans.rotation = Quat::from_axis_angle(Vec3::Y, rot.1) * Quat::from_axis_angle(-Vec3::X, rot.0);
}


pub fn kb_controls(input: Res<Input<KeyCode>>, mut q: Query<&mut Transform, With<PlayerCamera>>, t: Res<Time>) {
    const BASE_MOVEMENT: f32 = 2.0;

    let mut tfm = q.single_mut();
    let fwd = tfm.forward();
    let right = tfm.right();

    let travel = t.delta_seconds() * BASE_MOVEMENT;

    for code in input.get_pressed() {
        match code {
            KeyCode::W => tfm.translation += fwd * travel,
            KeyCode::A => tfm.translation -= right * travel,
            KeyCode::S => tfm.translation -= fwd * travel,
            KeyCode::D => tfm.translation += right * travel,
            KeyCode::Space => tfm.translation.y += travel,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => tfm.translation.y -= travel,

            _ => ()
        }
    }
}