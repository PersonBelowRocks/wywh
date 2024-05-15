// we put constants in their own file because the linter doesnt support this syntax so we
// confine the linter errors to this file only, making everything a lot less annoying

const CHUNK_OCCLUSION_BUFFER_SIZE: u32 = #{CHUNK_OCCLUSION_BUFFER_SIZE}u;
const CHUNK_OCCLUSION_BUFFER_DIMENSIONS: u32 = #{CHUNK_OCCLUSION_BUFFER_DIMENSIONS}u;

const ROTATION_MASK: u32 = #{ROTATION_MASK}u;
const ROTATION_SHIFT: u32 = #{ROTATION_SHIFT}u;
const FACE_MASK: u32 = #{FACE_MASK}u;
const FACE_SHIFT: u32 = #{FACE_SHIFT}u;

const FLIP_UV_X_BIT: u32 = #{FLIP_UV_X_BIT}u;
const FLIP_UV_Y_BIT: u32 = #{FLIP_UV_Y_BIT}u;

const HAS_NORMAL_MAP_BIT: u32 = #{HAS_NORMAL_MAP_BIT}u;

const DEFAULT_PBR_INPUT_FLAGS: u32 = #{DEFAULT_PBR_INPUT_FLAGS}u;
