use bevy::{
    diagnostic::{Diagnostic, DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use ve::topo::world::ChunkPos;
use ve::topo::{CHUNK_FULL_BLOCK_DIMS, fb_worldspace_to_chunkspace};
use ve::{
    diagnostics::ENGINE_DIAGNOSTICS,
    topo::{ActorLoadRegion, controller::PreviousActorPosition, world::VoxelRealm},
    util::sync::LockStrategy,
};
use voxel_engine::data::tile::Face;

use crate::controls::PlayerCamController;

#[derive(Component)]
pub struct DebugText;

#[derive(Component)]
pub struct FpsText;

pub fn update_debug_text(
    diagnostics: Res<DiagnosticsStore>,
    realm: VoxelRealm,
    mut debug_text_q: Query<&mut Text, With<DebugText>>,
    player_q: Query<&Transform, With<PlayerCamController>>,
) {
    let player = player_q.single().unwrap();
    let mut debug_text = debug_text_q.single_mut().unwrap();

    let pos = player.translation;
    let chunk_pos = ChunkPos::from(fb_worldspace_to_chunkspace(pos.floor().as_ivec3()));

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

    if let Some(gpu_update_time) = diagnostics
        .get(&ENGINE_DIAGNOSTICS.gpu_update_time)
        .and_then(Diagnostic::average)
    {
        sections.push(format!("gpu update time: {:.5}ms\n", gpu_update_time));
    }

    if let Some(mesh_extract_time) = diagnostics
        .get(&ENGINE_DIAGNOSTICS.mesh_extract_time)
        .and_then(Diagnostic::average)
    {
        sections.push(format!("mesh extract time: {:.5}ms\n", mesh_extract_time));
    }

    if let Some(mesh_build_time) = diagnostics
        .get(&ENGINE_DIAGNOSTICS.mesh_build_time)
        .and_then(Diagnostic::average)
    {
        sections.push(format!("mesh build time: {:.5}ms\n", mesh_build_time));
    }

    sections.push("\n".to_string());

    sections.push(format!("chunk: {}\n", chunk_pos));

    // let hr_load_reasons = realm
    //     .cm()
    //     .loaded_chunk(chunk_pos)
    //     .ok()
    //     .map(|cref| cref.cached_load_reasons())
    //     .map(|reasons| format!("{reasons:?}"))
    //     .unwrap_or_else(|| "NONE".to_string());

    let hr_load_reasons = "UNKNOWN";

    sections.push(format!("Load reasons: {hr_load_reasons}\n"));

    let hr_chunk_flags = realm
        .cm()
        .loaded_chunk(chunk_pos)
        .ok()
        .map(|cref| cref.flags(LockStrategy::Blocking).unwrap())
        .map(|flags| format!("{flags:?}"))
        .unwrap_or_else(|| "NONE".to_string());

    sections.push(format!("Chunk flags: {hr_chunk_flags}\n"));

    sections.push("\n".to_string());
    sections.push(format!("Tick: {}\n", realm.tick()));

    *debug_text = Text(sections.join(""));
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

pub fn chunk_borders(
    mut giz: Gizmos,
    observers: Query<&PreviousActorPosition, With<ActorLoadRegion>>,
) {
    for last_pos in &observers {
        let pos =
            last_pos.chunk_pos.worldspace_min().as_vec3() + (CHUNK_FULL_BLOCK_DIMS as f32 / 2.0);

        let gizmo_tf =
            Transform::from_translation(pos).with_scale(Vec3::splat(CHUNK_FULL_BLOCK_DIMS as _));
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
            *text = Text(format!("FPS: {value:>4.0}"));
        } else {
            // display "N/A" if we can't get a FPS measurement
            *text = Text("N/A".to_string());
        }
    }
}
