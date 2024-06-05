use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use ve::{
    render::meshing::controller::ExtractableChunkMeshData,
    topo::{
        controller::{ChunkPermitKey, LastPosition},
        world::VoxelRealm,
        ChunkObserver,
    },
    util::ws_to_chunk_pos,
};
use voxel_engine::{data::tile::Face, topo::world::Chunk};

use crate::camera::PlayerCamController;

#[derive(Component)]
pub struct SpatialDebugText;

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

pub fn update_spatial_debug_text(
    realm: VoxelRealm,
    meshes: Res<ExtractableChunkMeshData>,
    mut q: Query<&mut Text, With<SpatialDebugText>>,
    player_q: Query<&Transform, With<PlayerCamController>>,
) {
    let pos = player_q.single().translation;
    let chunk_pos = ws_to_chunk_pos(pos.floor().as_ivec3());

    let permit_flags = realm
        .permits()
        .get(ChunkPermitKey::Chunk(chunk_pos))
        .map(|permit| permit.flags);

    let load_reasons = realm
        .cm()
        .get_loaded_chunk(chunk_pos, true)
        .ok()
        .map(|cref| cref.load_reasons());

    let chunk_flags = realm
        .cm()
        .get_loaded_chunk(chunk_pos, true)
        .ok()
        .map(|cref| cref.flags());

    let mesh = meshes.active.get(chunk_pos);

    for mut text in &mut q {
        text.sections = [
            format!("x: {:.5}\n", pos.x),
            format!("y: {:.5}\n", pos.y),
            format!("z: {:.5}\n", pos.z),
            format!("chunk: {}\n", chunk_pos),
            format!("load reasons: {load_reasons:?}\n"),
            format!("chunk flags: {chunk_flags:?}\n"),
            format!("permit flags: {permit_flags:?}\n"),
            format!("\n"),
            format!("mesh: {mesh:?}"),
        ]
        .map(text_section)
        .to_vec();
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

pub fn chunk_borders(mut giz: Gizmos, observers: Query<&LastPosition, With<ChunkObserver>>) {
    for last_pos in &observers {
        let pos = last_pos.chunk_pos.worldspace_min().as_vec3() + (Chunk::SIZE as f32 / 2.0);

        let gizmo_tf = Transform::from_translation(pos).with_scale(Vec3::splat(Chunk::SIZE as _));
        giz.cuboid(gizmo_tf, Color::LIME_GREEN);
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
            text.sections[1].value = format!("{value:>4.0}");
        } else {
            // display "N/A" if we can't get a FPS measurement
            // add an extra space to preserve alignment
            text.sections[1].value = " N/A".into();
            text.sections[1].style.color = Color::WHITE;
        }
    }
}
