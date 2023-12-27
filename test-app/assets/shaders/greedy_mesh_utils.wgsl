#import bevy_pbr::{
    pbr_types,
    pbr_bindings,
    pbr_functions,
    pbr_fragment,
    mesh_view_bindings as view_bindings,
    mesh_view_types,
    lighting,
    transmission,
    clustered_forward as clustering,
    shadows,
    ambient,
    mesh_types::{MESH_FLAGS_SHADOW_RECEIVER_BIT, MESH_FLAGS_TRANSMITTED_SHADOW_RECEIVER_BIT},
    utils::E,
    prepass_utils,
    mesh_bindings::mesh,
    mesh_view_bindings::view,
    parallax_mapping::parallaxed_uv,
    forward_io::VertexOutput,
}

#ifdef SCREEN_SPACE_AMBIENT_OCCLUSION
#import bevy_pbr::mesh_view_bindings::screen_space_ambient_occlusion_texture
#import bevy_pbr::gtao_utils::gtao_multibounce
#endif

#ifdef ENVIRONMENT_MAP
#import bevy_pbr::environment_map
#endif

#import bevy_core_pipeline::tonemapping::{screen_space_dither, powsafe, tone_mapping}

struct FaceTexture {
    flags: u32,
    color_tex_pos: vec2<f32>,
    normal_tex_pos: vec2<f32>,
}

struct GreedyVertexOutput {
    // This is `clip position` when the struct is used as a vertex stage output
    // and `frag coord` when used as a fragment stage input
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) world_tangent: vec4<f32>,

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    @location(5) @interpolate(flat) instance_index: u32,
#endif
    @location(10) @interpolate(flat) texture_id: u32,
    @location(11) @interpolate(flat) texture_rot: f32,

    @location(12) @interpolate(flat) flip_uv_x: u32,
    @location(13) @interpolate(flat) flip_uv_y: u32,

    @location(14) occlusion: f32,
}

fn preprocess_greedy_vertex_output(raw_in: GreedyVertexOutput) -> GreedyVertexOutput {
    var in: GreedyVertexOutput = raw_in;

    let fract_uv = fract(in.uv);
    var uv: vec2<f32> = fract_uv;

    if in.flip_uv_x != 0u {
        uv.x = (1.0 - uv.x);
    }

    if in.flip_uv_y != 0u {
        uv.y = (1.0 - uv.y);
    }

    let a = in.texture_rot;
    let M = mat2x2(
        cos(a), -sin(a),
        sin(a),  cos(a)
    );

    let offset = vec2(0.5, 0.5);
    in.uv = (M * (uv - offset)) + offset;

    in.occlusion = raw_in.occlusion;

    return in;
}

fn pbr_input_from_greedy_vertex_output(
    in: GreedyVertexOutput,
    is_front: bool,
    double_sided: bool,
) -> pbr_types::PbrInput {
    var pbr_input: pbr_types::PbrInput = pbr_types::pbr_input_new();

    pbr_input.flags = mesh[in.instance_index].flags;
    pbr_input.is_orthographic = view.projection[3].w == 1.0;
    pbr_input.V = pbr_functions::calculate_view(in.world_position, pbr_input.is_orthographic);
    pbr_input.frag_coord = in.position;
    pbr_input.world_position = in.world_position;

#ifdef VERTEX_COLORS
    pbr_input.material.base_color = in.color;
#endif

    pbr_input.world_normal = pbr_functions::prepare_world_normal(
        in.world_normal,
        double_sided,
        is_front,
    );

#ifdef LOAD_PREPASS_NORMALS
    pbr_input.N = prepass_utils::prepass_normal(in.position, 0u);
#else
    pbr_input.N = normalize(pbr_input.world_normal);
#endif

    return pbr_input;
}

const HAS_NORMAL_MAP_BIT: u32 = #{HAS_NORMAL_MAP_BIT}u;

fn greedy_mesh_pbr_input(
    in: GreedyVertexOutput,
    is_front: bool,
    face: FaceTexture,
    scale: f32,
) -> pbr_types::PbrInput {
    let double_sided = (pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT) != 0u;

    var pbr_input: pbr_types::PbrInput = pbr_input_from_greedy_vertex_output(in, is_front, double_sided);
    pbr_input.material.flags = pbr_bindings::material.flags;
    pbr_input.material.base_color *= pbr_bindings::material.base_color;
    pbr_input.material.deferred_lighting_pass_id = pbr_bindings::material.deferred_lighting_pass_id;

    var uv = in.uv;

    if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_DEPTH_MAP_BIT) != 0u) {
        let V = pbr_input.V;
        let N = in.world_normal;
        let T = in.world_tangent.xyz;
        let B = in.world_tangent.w * cross(N, T);
        // Transform V from fragment to camera in world space to tangent space.
        let Vt = vec3(dot(V, T), dot(V, B), dot(V, N));
        uv = parallaxed_uv(
            pbr_bindings::material.parallax_depth_scale,
            pbr_bindings::material.max_parallax_layer_count,
            pbr_bindings::material.max_relief_mapping_search_steps,
            uv,
            // Flip the direction of Vt to go toward the surface to make the
            // parallax mapping algorithm easier to understand and reason
            // about.
            -Vt,
        );
    }

    if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u) {
        let color_uv = ((uv / scale) + (face.color_tex_pos / scale) / scale);
        pbr_input.material.base_color *= textureSampleBias(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, color_uv, view.mip_bias);
    }

    pbr_input.material.flags = pbr_bindings::material.flags;

    // NOTE: Unlit bit not set means == 0 is true, so the true case is if lit
    if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u) {
        pbr_input.material.reflectance = pbr_bindings::material.reflectance;
        pbr_input.material.ior = pbr_bindings::material.ior;
        pbr_input.material.attenuation_color = pbr_bindings::material.attenuation_color;
        pbr_input.material.attenuation_distance = pbr_bindings::material.attenuation_distance;
        pbr_input.material.alpha_cutoff = pbr_bindings::material.alpha_cutoff;

        // emissive
        // TODO use .a for exposure compensation in HDR
        var emissive: vec4<f32> = pbr_bindings::material.emissive;
        if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT) != 0u) {
            emissive = vec4<f32>(emissive.rgb * textureSampleBias(pbr_bindings::emissive_texture, pbr_bindings::emissive_sampler, uv, view.mip_bias).rgb, 1.0);
        }
        pbr_input.material.emissive = emissive;

        // metallic and perceptual roughness
        var metallic: f32 = pbr_bindings::material.metallic;
        var perceptual_roughness: f32 = pbr_bindings::material.perceptual_roughness;

        if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_METALLIC_ROUGHNESS_TEXTURE_BIT) != 0u) {
            let metallic_roughness = textureSampleBias(pbr_bindings::metallic_roughness_texture, pbr_bindings::metallic_roughness_sampler, uv, view.mip_bias);
            // Sampling from GLTF standard channels for now
            metallic *= metallic_roughness.b;
            perceptual_roughness *= metallic_roughness.g;
        }

        pbr_input.material.metallic = metallic;
        pbr_input.material.perceptual_roughness = perceptual_roughness;

        var specular_transmission: f32 = pbr_bindings::material.specular_transmission;
        pbr_input.material.specular_transmission = specular_transmission;

        var thickness: f32 = pbr_bindings::material.thickness;
        // scale thickness, accounting for non-uniform scaling (e.g. a “squished” mesh)
        thickness *= length(
            (transpose(mesh[in.instance_index].model) * vec4(pbr_input.N, 0.0)).xyz
        );
        pbr_input.material.thickness = thickness;

        var diffuse_transmission = pbr_bindings::material.diffuse_transmission;
        pbr_input.material.diffuse_transmission = diffuse_transmission;

        // occlusion
        // TODO: Split into diffuse/specular occlusion?
        var occlusion: vec3<f32> = vec3(1.0);

#ifdef SCREEN_SPACE_AMBIENT_OCCLUSION
        let ssao = textureLoad(screen_space_ambient_occlusion_texture, vec2<i32>(in.position.xy), 0i).r;
        let ssao_multibounce = gtao_multibounce(ssao, pbr_input.material.base_color.rgb);
        occlusion = min(occlusion, ssao_multibounce);
#endif
        pbr_input.occlusion = occlusion;

        // N (normal vector)
//#ifndef LOAD_PREPASS_NORMALS
        var normal_map_uv = uv;

        if (face.flags & HAS_NORMAL_MAP_BIT) != 0u {
            normal_map_uv = ((uv / scale) + (face.normal_tex_pos / scale) / scale);

            pbr_input.N = pbr_functions::apply_normal_mapping(
                pbr_bindings::material.flags,
                pbr_input.world_normal,
                double_sided,
                is_front,
                in.world_tangent,
                normal_map_uv,
                view.mip_bias,
            );
        } else {
            pbr_input.N = pbr_input.world_normal;
        }

        
//#endif
    }

    return pbr_input;
}

fn occlusion_curve(o: f32) -> f32 {
    let unclamped = (0.1 / (-o + 1.11)) - 0.25;
    return clamp(unclamped, 0.0, 1.0);
}