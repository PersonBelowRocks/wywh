use std::f32::consts::PI;

use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;

use bevy::window::CursorGrabMode;

#[derive(Component, Default, Copy, Clone)]
pub struct PlayerCamController {
    pub controlled: bool,
}

#[derive(Component, Default, Copy, Clone)]
pub struct PlayerHeadlight;

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

pub(crate) fn mouse_controls(
    mut rot: Local<Rotation>,
    mut events: EventReader<MouseMotion>,
    mut player: Query<(&mut Transform, &PlayerCamController)>,
) {
    const SENSITIVITY: f32 = 0.05;

    let (mut trans, controller) = player.single_mut();

    if !controller.controlled {
        return;
    }

    let mut pitch: f32 = 0.0;
    let mut yaw: f32 = 0.0;

    for mouse in events.read() {
        pitch += mouse.delta.y * SENSITIVITY;
        yaw += -mouse.delta.x * SENSITIVITY;
    }

    let new_pitch = rot.0 + (pitch * PI / 180.0);
    if (-PI / 2.0..=PI / 2.0).contains(&new_pitch) {
        rot.0 = new_pitch;
    }
    rot.1 += yaw * PI / 180.0;

    trans.rotation = Quat::from_axis_angle(Vec3::Y, rot.1) * Quat::from_axis_angle(-Vec3::X, rot.0);
}

pub fn kb_controls(
    input: Res<ButtonInput<KeyCode>>,
    mut q: Query<(&mut Transform, &PlayerCamController)>,
    t: Res<Time>,
) {
    const BASE_MOVEMENT: f32 = 12.0;

    let (mut tfm, controller) = q.single_mut();

    if !controller.controlled {
        return;
    }

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

            _ => (),
        }
    }
}

pub fn cursor_grab(
    mut q_window: Query<&mut Window>,
    mut q_controller: Query<&mut PlayerCamController>,
    btn: Res<ButtonInput<MouseButton>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    let mut window = q_window.single_mut();
    let mut controller = q_controller.single_mut();

    if btn.just_pressed(MouseButton::Left) {
        // if you want to use the cursor, but not let it leave the window,
        // use `Confined` mode:
        info!("Locking cursor");
        window.cursor.grab_mode = CursorGrabMode::Locked;
        window.cursor.visible = false;
        controller.controlled = true;
        // window.set_cursor_grab_mode(CursorGrabMode::Confined);

        // for a game that doesn't use the cursor (like a shooter):
        // use `Locked` mode to keep the cursor in one place
        // window.set_cursor_grab_mode(CursorGrabMode::Locked);
        // also hide the cursor
        // window.set_cursor_visibility(false);
    }

    if key.just_pressed(KeyCode::Escape) {
        info!("Unlocking cursor");
        window.cursor.grab_mode = CursorGrabMode::None;
        window.cursor.visible = true;
        controller.controlled = false;
    }
}
