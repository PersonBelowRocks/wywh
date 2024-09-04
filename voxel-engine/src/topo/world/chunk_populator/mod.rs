use bevy::prelude::*;

use crate::topo::controller::PopulateChunkEvent;

pub fn handle_population_events(population_events: EventReader<PopulateChunkEvent>) {}
