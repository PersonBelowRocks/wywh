use std::thread::JoinHandle;

use bevy::prelude::error;
use bevy::prelude::Asset;
use bevy::prelude::Material;
use bevy::prelude::Mesh;
use bevy::prelude::Resource;

use bevy::prelude::Vec3;
use bevy::render::mesh::Indices;

use bevy::render::render_resource::PrimitiveTopology;
use cb::channel::Receiver;

use cb::channel::Sender;

use crate::data::registries::Registries;
use crate::render::meshing::greedy::material::GreedyMeshMaterial;
use crate::topo::access::ChunkBounds;
use crate::topo::access::ReadAccess;

use crate::topo::chunk::ChunkPos;
use crate::topo::chunk_ref::ChunkRef;

use crate::topo::chunk_ref::ChunkVoxelOutput;

use super::adjacency::AdjacentTransparency;
use super::error::MesherError;
use super::mesh::ChunkMesh;
use super::occlusion::ChunkOcclusionMap;

#[derive(Clone, Debug)]
pub struct MesherOutput {
    pub mesh: Mesh,
    pub occlusion: ChunkOcclusionMap,
}

pub struct Context<'a> {
    pub adjacency: &'a AdjacentTransparency,
    pub registries: &'a Registries,
}

pub trait Mesher: Clone + Send + Sync + 'static {
    type Material: Material + Asset;

    fn build<Acc>(
        &self,
        access: &Acc,
        adjacency: Context,
    ) -> Result<MesherOutput, MesherError<Acc::ReadErr>>
    where
        Acc: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds;

    fn material(&self) -> Self::Material;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, dm::From, dm::Into)]
pub struct MeshingTaskId(u32);

pub(crate) struct BuildMeshCommand {
    pub chunk_ref: ChunkRef,
    pub adjacency: Box<AdjacentTransparency>,
    pub id: MeshingTaskId,
}

pub(crate) enum MesherWorkerCommand {
    Build(BuildMeshCommand),
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct MesherWorkerOutput {
    pub pos: ChunkPos,
    pub id: MeshingTaskId,
    pub output: MesherOutput,
}

pub(crate) struct MesherWorker {
    handle: JoinHandle<()>,
}

impl MesherWorker {
    pub fn spawn<Mat: Material>(
        cmd_receiver: &Receiver<MesherWorkerCommand>,
        mesh_sender: &Sender<MesherWorkerOutput>,
        mesher: &impl Mesher<Material = Mat>,
        registries: Registries,
    ) -> Self {
        let cmd_receiver = cmd_receiver.clone();
        let mesh_sender = mesh_sender.clone();
        let mesher = mesher.clone();

        let handle = std::thread::spawn(move || {
            let mut interrupt = false;
            while !interrupt {
                // TODO: error handling
                let cmd = cmd_receiver.recv().unwrap_or_else(|err| {
                    error!("MesherWorker errored when receiving from command channel and is shutting down. Error: {err:?}");
                    MesherWorkerCommand::Shutdown
                });

                match cmd {
                    MesherWorkerCommand::Shutdown => interrupt = true,
                    MesherWorkerCommand::Build(data) => {
                        // TODO: error handling
                        let mesh = data
                            .chunk_ref
                            .with_read_access(|access| {
                                let cx = Context {
                                    adjacency: &data.adjacency,
                                    registries: &registries,
                                };

                                mesher.build(&access, cx).unwrap()
                            })
                            .unwrap();

                        mesh_sender
                            .send(MesherWorkerOutput {
                                pos: data.chunk_ref.pos(),
                                id: data.id,
                                output: mesh,
                            })
                            .unwrap();
                    }
                }
            }
        });

        Self { handle }
    }
}

#[derive(Resource)]
pub struct ParallelMeshBuilder<HQM: Mesher, LQM: Mesher> {
    workers: Vec<MesherWorker>,
    cmd_sender: Sender<MesherWorkerCommand>,
    mesh_receiver: Receiver<MesherWorkerOutput>,
    pending_tasks: hb::HashSet<MeshingTaskId>,
    registries: Registries,
    hq_mesher: HQM,
    lq_mesher: LQM,
}

impl<HQM: Mesher, LQM: Mesher> ParallelMeshBuilder<HQM, LQM> {
    fn spawn_workers(
        number: u32,
        cmd_recv: &Receiver<MesherWorkerCommand>,
        mesh_send: &Sender<MesherWorkerOutput>,
        mesher: &HQM,
        registries: Registries,
    ) -> Vec<MesherWorker> {
        let mut workers = Vec::new();

        for _ in 0..number {
            let worker = MesherWorker::spawn(cmd_recv, mesh_send, mesher, registries.clone());
            workers.push(worker);
        }

        workers
    }

    pub fn new(hq_mesher: HQM, lq_mesher: LQM, registries: Registries) -> Self {
        let num_cpus: usize = std::thread::available_parallelism().unwrap().into();

        // TODO: create these channels in Self::spawn_workers instead
        let (cmd_send, cmd_recv) = cb::channel::unbounded::<MesherWorkerCommand>();
        let (mesh_send, mesh_recv) = cb::channel::unbounded::<MesherWorkerOutput>();

        Self {
            workers: Self::spawn_workers(
                num_cpus as _,
                &cmd_recv,
                &mesh_send,
                &hq_mesher,
                registries.clone(),
            ),
            cmd_sender: cmd_send,
            mesh_receiver: mesh_recv,
            pending_tasks: hb::HashSet::new(),
            registries,
            hq_mesher,
            lq_mesher,
        }
    }

    fn unique_task_id(&self) -> MeshingTaskId {
        let max: u32 = self
            .pending_tasks
            .iter()
            .max()
            .cloned()
            .unwrap_or(0.into())
            .into();
        for id in 0..=(max + 1) {
            if !self.pending_tasks.contains(&MeshingTaskId::from(id)) {
                return id.into();
            }
        }

        panic!("Good luck queuing this many tasks lol");
    }

    fn send_cmd(&self, cmd: MesherWorkerCommand) {
        // TODO: error handling
        self.cmd_sender.send(cmd).unwrap()
    }

    fn add_pending_task(&mut self, id: MeshingTaskId) {
        self.pending_tasks.insert(id);
    }

    fn remove_pending_task(&mut self, id: MeshingTaskId) -> bool {
        self.pending_tasks.remove(&id)
    }

    pub fn queue_chunk(
        &mut self,
        chunk_ref: ChunkRef,
        adjacency: AdjacentTransparency,
    ) -> MeshingTaskId {
        let id = self.unique_task_id();
        self.add_pending_task(id);

        let build_cmd = BuildMeshCommand {
            id,
            chunk_ref,
            adjacency: Box::new(adjacency),
        };

        let cmd = MesherWorkerCommand::Build(build_cmd);
        self.send_cmd(cmd);

        id
    }

    // TODO: make this return an iterator instead
    pub fn finished_meshes(&mut self) -> MesherResults<'_> {
        MesherResults {
            pending_tasks: &mut self.pending_tasks,
            recv: &mut self.mesh_receiver,
        }
    }

    pub fn shutdown(self) {
        for _ in 0..self.workers.len() {
            self.send_cmd(MesherWorkerCommand::Shutdown);
        }

        for worker in self.workers.into_iter() {
            worker.handle.join().unwrap();
        }
    }

    pub fn lq_material(&self) -> LQM::Material {
        self.lq_mesher.material()
    }

    pub fn hq_material(&self) -> HQM::Material {
        self.hq_mesher.material()
    }
}

pub struct MesherResults<'a> {
    pending_tasks: &'a mut hb::HashSet<MeshingTaskId>,
    recv: &'a mut Receiver<MesherWorkerOutput>,
}

impl<'a> Iterator for MesherResults<'a> {
    type Item = MesherWorkerOutput;

    fn next(&mut self) -> Option<Self::Item> {
        self.recv.try_recv().ok().map(|output| {
            self.pending_tasks.remove(&output.id);
            output
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct ChunkMeshAttributes {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub textures: Vec<u32>,
    pub misc_data: Vec<u32>,
    pub indices: Vec<u32>,
}

impl ChunkMeshAttributes {
    pub fn to_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

        mesh.set_indices(Some(Indices::U32(self.indices)));

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(GreedyMeshMaterial::TEXTURE_MESH_ATTR, self.textures);
        mesh.insert_attribute(GreedyMeshMaterial::MISC_DATA_ATTR, self.misc_data);

        mesh
    }
}
