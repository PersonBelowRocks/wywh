use std::{sync::Arc, time::Instant};

use bevy::{
    diagnostic::{
        Diagnostic, DiagnosticMeasurement, DiagnosticPath, DiagnosticsStore, RegisterDiagnostic,
    },
    prelude::*,
    render::RenderApp,
};
use cb::channel::*;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DiagRecStatus {
    Record,
    Ignore,
    Invalid,
}

#[derive(Clone, Resource)]
pub struct DiagnosticsTx(hb::HashMap<DiagnosticPath, Sender<DiagnosticMeasurement>>);

impl DiagnosticsTx {
    pub fn new<'a>(
        pairs: impl IntoIterator<Item = (DiagnosticPath, Sender<DiagnosticMeasurement>)>,
    ) -> Self {
        Self(hb::HashMap::from_iter(pairs.into_iter()))
    }

    pub fn contains(&self, path: &DiagnosticPath) -> bool {
        self.0.contains_key(path)
    }

    pub fn measure<T, F: for<'a> FnOnce(&mut DiagRecStatus) -> T>(
        &self,
        path: &DiagnosticPath,
        f: F,
    ) -> T {
        match self.0.get(path) {
            Some(diagnostic) => {
                let mut status = DiagRecStatus::Record;
                let then = Instant::now();

                let out = f(&mut status);

                if status == DiagRecStatus::Record {
                    let now = Instant::now();
                    let elapsed_millis = (now - then).as_secs_f64() * 1000.0;
                    diagnostic
                        .try_send(DiagnosticMeasurement {
                            time: now,
                            value: elapsed_millis,
                        })
                        .ok();
                }

                out
            }
            None => {
                error!("Could not find diagnostic '{path}' in diagnostics sender");
                f(&mut DiagRecStatus::Invalid)
            }
        }
    }
}

#[derive(Clone, Resource)]
struct DiagnosticsRx(Arc<hb::HashMap<DiagnosticPath, Receiver<DiagnosticMeasurement>>>);

impl DiagnosticsRx {
    pub fn new<'a>(
        pairs: impl IntoIterator<Item = (DiagnosticPath, Receiver<DiagnosticMeasurement>)>,
    ) -> Self {
        Self(Arc::new(hb::HashMap::from_iter(pairs.into_iter())))
    }

    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&DiagnosticPath, &Receiver<DiagnosticMeasurement>)> {
        self.0.iter()
    }
}

fn create_channels(paths: &[DiagnosticPath]) -> (DiagnosticsTx, DiagnosticsRx) {
    let mut senders = Vec::new();
    let mut receivers = Vec::new();

    for path in paths {
        let (tx, rx) = bounded::<DiagnosticMeasurement>(1);

        senders.push((path.clone(), tx));
        receivers.push((path.clone(), rx));
    }

    (DiagnosticsTx::new(senders), DiagnosticsRx::new(receivers))
}

fn collect(receiver: &Receiver<DiagnosticMeasurement>, diagnostic: &mut Diagnostic) {
    while let Some(measurement) = receiver.try_recv().ok() {
        diagnostic.add_measurement(measurement)
    }
}

fn collect_diagnostics(rx: Res<DiagnosticsRx>, mut diagnostics: ResMut<DiagnosticsStore>) {
    for (path, receiver) in rx.iter() {
        let Some(diagnostic) = diagnostics.get_mut(path) else {
            error!("Could not find diagnostic 'path' in diagnostics store");
            continue;
        };

        collect(receiver, diagnostic);
    }
}

pub struct VoxelEngineDiagnostics {
    pub gpu_update_time: DiagnosticPath,
    pub mesh_extract_time: DiagnosticPath,
}

pub const ENGINE_DIAGNOSTICS: VoxelEngineDiagnostics = VoxelEngineDiagnostics {
    gpu_update_time: DiagnosticPath::const_new("gpu_update_time"),
    mesh_extract_time: DiagnosticPath::const_new("mesh_extract_time"),
};

pub struct VoxelEngineDiagnosticsPlugin;

impl Plugin for VoxelEngineDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        let (diag_tx, diag_rx) = create_channels(&[
            ENGINE_DIAGNOSTICS.gpu_update_time,
            ENGINE_DIAGNOSTICS.mesh_extract_time,
        ]);

        app.register_diagnostic(
            Diagnostic::new(ENGINE_DIAGNOSTICS.gpu_update_time)
                .with_suffix("ms")
                .with_max_history_length(10),
        )
        .register_diagnostic(
            Diagnostic::new(ENGINE_DIAGNOSTICS.mesh_extract_time)
                .with_suffix("ms")
                .with_max_history_length(10),
        );

        app.insert_resource(diag_tx.clone())
            .insert_resource(diag_rx)
            .add_systems(PreUpdate, collect_diagnostics);

        app.sub_app_mut(RenderApp).insert_resource(diag_tx);
    }
}
