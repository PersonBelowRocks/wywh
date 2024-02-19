use bevy::{
    math::{ivec3, primitives::Primitive3d, vec3},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef},
};
use mip_texture_array::{asset::MippedArrayTexture, MippedArrayTexturePlugin, MippedArrayTextureBuilder};

#[derive(Resource)]
struct TexarrTextures(Vec<Handle<Image>>);

#[derive(States, Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum AppState {
    #[default]
    Loading,
    Loaded,
}

#[derive(Component, Copy, Clone)]
struct TestingTextureArray;

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins,
        MippedArrayTexturePlugin {
            inject_into_render_images: true,
        },
        MaterialPlugin::<TestingMaterial>::default(),
    ));

    app.add_state::<AppState>();

    app.add_systems(Startup, setup)
        .add_systems(Update, loading.run_if(in_state(AppState::Loading)))
        .add_systems(OnEnter(AppState::Loaded), insert_example);

    app.run()
}

fn setup(mut cmds: Commands, server: Res<AssetServer>) {
    let textures = vec![
        server.load::<Image>("first.png"),
        server.load::<Image>("second.png"),
    ];

    cmds.insert_resource(TexarrTextures(textures));
}

fn loading(
    mut next_state: ResMut<NextState<AppState>>,
    server: Res<AssetServer>,
    texarr_imgs: Res<TexarrTextures>,
) {
    for handle in texarr_imgs.0.iter() {
        if !server.is_loaded_with_dependencies(handle) {
            return;
        }
    }

    next_state.set(AppState::Loaded);
}

fn insert_example(
    mut cmds: Commands,
    handles: Res<TexarrTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TestingMaterial>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut array_textures: ResMut<Assets<MippedArrayTexture>>,
) {
    let mut builder = MippedArrayTextureBuilder::new(16);

    for handle in handles.0.iter() {
        builder.add_image(handle.clone(), images.as_ref()).unwrap();
    }

    let texarr_handle = builder
        .finish(images.as_mut(), array_textures.as_mut())
        .unwrap();

    for mip in 0..4 {
        cmds.spawn(MaterialMeshBundle::<TestingMaterial> {
            transform: Transform::from_translation(vec3(0.0, 0.0, (mip as f32) * 2.1)),
            mesh: meshes.add(shape::Plane::from_size(2.0).into()),
            material: materials.add(TestingMaterial {
                tex: texarr_handle.clone().untyped().typed_unchecked::<Image>(),
                array_idx: 0,
                mip_level: mip,
            }),
            ..default()
        });
    }

    cmds.spawn(Camera3dBundle {
        transform: Transform::from_translation(vec3(0.0, 10.0, 4.0))
            .looking_at(vec3(0.0, 0.0, 4.0), Vec3::X),
        ..default()
    });

    cmds.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.5,
    })
}

#[derive(Clone, Asset, TypePath, AsBindGroup, Debug)]
struct TestingMaterial {
    #[texture(0, dimension = "2d_array")]
    #[sampler(1)]
    tex: Handle<Image>,
    #[uniform(2)]
    mip_level: u32,
    #[uniform(3)]
    array_idx: u32,
}

impl Material for TestingMaterial {
    fn fragment_shader() -> ShaderRef {
        "testing_frag.wgsl".into()
    }
}
