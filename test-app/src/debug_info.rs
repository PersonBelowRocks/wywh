use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use ve::{
    render::meshing::controller::ExtractableChunkMeshData,
    topo::{controller::LastPosition, world::VoxelRealm, ObserverSettings},
    util::ws_to_chunk_pos,
};
use voxel_engine::{data::tile::Face, topo::world::Chunk};

use crate::controls::PlayerCamController;

#[derive(Component)]
pub struct DebugText;

#[derive(Component)]
pub struct DirectionText;

#[derive(Component)]
pub struct FpsText;

pub fn text_section(string: impl Into<String>) -> TextSection {
    let default_style = TextStyle {
        font_size: 35.0,
        color: Color::WHITE,
        ..default()
    };

    TextSection::new(string, default_style)
}

pub fn update_debug_text(
    realm: VoxelRealm,
    meshes: Res<ExtractableChunkMeshData>,
    mut q: Query<&mut Text, With<DebugText>>,
    player_q: Query<&Transform, With<PlayerCamController>>,
) {
    let player = player_q.single();
    let pos = player.translation;
    let chunk_pos = ws_to_chunk_pos(pos.floor().as_ivec3());

    let mut sections = Vec::<String>::new();

    let cardinal = get_cardinal_direction(player.forward());
    let direction_letter = match cardinal {
        Face::North => "N",
        Face::East => "E",
        Face::South => "S",
        Face::West => "W",

        _ => panic!("Unexpected cardinal direction"),
    };

    sections.push(format!("Facing: {direction_letter}\n"));

    sections.extend_from_slice(&[
        format!("x: {:.5}\n", pos.x),
        format!("y: {:.5}\n", pos.y),
        format!("z: {:.5}\n", pos.z),
    ]);

    sections.push("\n".to_string());
    sections.push(format!("chunk: {}\n", chunk_pos));

    let hr_load_reasons = realm
        .cm()
        .get_loaded_chunk(chunk_pos, true)
        .ok()
        .map(|cref| cref.cached_load_reasons())
        .map(|reasons| format!("{reasons:?}"))
        .unwrap_or_else(|| "NONE".to_string());

    sections.push(format!("Load reasons: {hr_load_reasons}\n"));

    let hr_chunk_flags = realm
        .cm()
        .get_loaded_chunk(chunk_pos, true)
        .ok()
        .map(|cref| cref.flags())
        .map(|flags| format!("{flags:?}"))
        .unwrap_or_else(|| "NONE".to_string());

    sections.push(format!("Chunk flags: {hr_chunk_flags}\n"));

    let statuses = meshes.get_statuses(chunk_pos);

    if !statuses.is_empty() {
        sections.push("Chunk mesh statuses:\n".to_string());

        for (lod, status) in statuses.iter() {
            sections.push(format!(" - {lod:?} : {:?}\n", status.status));
        }
    } else {
        sections.push("No LODs where this chunk has a mesh status\n".to_string())
    }

    sections.push("\n".to_string());
    sections.push(format!("Tick: {}\n", realm.tick()));

    for mut text in &mut q {
        text.sections = sections.clone().into_iter().map(text_section).collect();
    }
}

pub fn get_cardinal_direction(dir: Dir3) -> Face {
    let fwd_xz = dir.xz();

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
}

pub fn chunk_borders(mut giz: Gizmos, observers: Query<&LastPosition, With<ObserverSettings>>) {
    for last_pos in &observers {
        let pos = last_pos.chunk_pos.worldspace_min().as_vec3() + (Chunk::SIZE as f32 / 2.0);

        let gizmo_tf = Transform::from_translation(pos).with_scale(Vec3::splat(Chunk::SIZE as _));
        giz.cuboid(gizmo_tf, Color::srgb(1.0, 0.33, 0.33));
    }
}

pub fn fps_text_update_system(
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<&mut Text, With<FpsText>>,
) {
    for mut text in &mut query {
        // try to get a "smoothed" FPS value from Bevy
        if let Some(value) = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|fps| fps.smoothed())
        {
            // Format the number as to leave space for 4 digits, just in case,
            // right-aligned and rounded. This helps readability when the
            // number changes rapidly.
            text.sections = [format!("FPS: {value:>4.0}")].map(text_section).to_vec();
        } else {
            // display "N/A" if we can't get a FPS measurement
            text.sections = [format!("N/A")].map(text_section).to_vec();
        }
    }
}
