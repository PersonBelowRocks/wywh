# Wish You Were Here

This is a very WIP voxel game made in bevy. The game will eventually be a sandbox survival game (like Minecraft) mixed with a logistics/automation game (like Factorio or Satisfactory), with exploration elements (back to Minecraft again). The concept (as vague and loose as it is) is largely inspired by modded Minecraft.

Development is currently largely focused on technical stuff so that's what this readme will mostly cover.

## Engine

The game is built in [bevy](https://github.com/bevyengine/bevy) and uses a custom built voxel engine, which is where development is largely focused at the moment. The voxel engine is also built in bevy and takes the form of a plugin.

### Engine design

The engine is supposed to be a chunked and rasterized voxel engine, like Minecraft. This type of engine has the advantage of being both simple and flexible, and is probably the most common way of making voxel games. There's plenty of other projects to take inspiration from out there so many technical things (like greedy meshing) are already sorted out and there's good articles out there explaining such things.

On top of being rasterized and chunked, this engine is supposed to make heavy use of concurrency and allow things made with the voxel engine to also be very concurrent. At the moment the main way that this is achieved is by making every chunk sit behind a read-write lock.

Controlling the engine's loading and rendering of chunks is largely done through bevy's ECS, especially by sending and reading events.

#### Controllers

Controllers are bits of the engine that are in charge of a specific thing. For example rendering and building meshes for chunks, generating terrain, and loading/unloading chunks. They take the form of bevy plugins but aren't meant to be used outside of the voxel engine. Using plugins this way lets us split up the logic and code into smaller bits and makes development a whole lot easier.

#### Chunks, Chunk Management and ECS Permits

Chunks are how the engine splits up voxel data into smaller parts that can be processed individually and parallel with eachother. Under the hood chunks are mainly a big 3d array of indices into a palette (a 1d vector). They're designed to have fast reads but slower writes. This is because you usually end up reading voxel data more than writing voxel data.

In order to be worked with chunks usually need to exist in memory in some form. A chunk existing in memory is in a "loaded" state. Not existing in memory means a chunk is "unloaded". Loading and unloading of chunks is quite complex because there's a lot of things that require chunks to be loaded, but we can't keep everything loaded all the time because we need to keep memory usage down. To simplify this problem the engine has a concept of "load reasons". Chunks need to have a reason to be loaded. If a chunk has no reason to be loaded, it will be automatically unloaded. These reasons are updated through ECS events, and these events are in turn both dispatched by the voxel engine and the user of the engine. At the moment load reasons are represented as simple bitflags but this might change in the future to allow for more complex and customizable logic. Loading and unloading of chunks is handled by the "world controller".

Often there's reason to work with chunks as ECS entities. For example for rendering or collision logic. But not every chunk needs to be an ECS entity, and we shouldn't store chunk voxel data as a component on an entity because we'd have to iterate through lots of entities to find a specific chunk. Additionally, we might want to have a chunk ECS entity without the chunk being loaded. To solve this the engine has something called "ECS permits". ECS permits are sort of like load reasons but are reasons for a chunk having an associated entity rather than being loaded. If a chunk doesn't have an ECS permit, it's not allowed to have an ECS entity. These permits are also updated through events much like chunk load reasons. In addition to functioning like load reasons, permits are stored in a big multi-key hashmap indexable by both a bevy `Entity` type and a chunk position. This means that there's a 1 to 1 relationship between a chunk position and an entity, and lets us get a specific chunk entity through a position (or vice versa) in O(1). A chunk can have an ECS entity associated with it without it being loaded. For example if we're just rendering a chunk but don't care about its voxel data we can have a chunk entity (and permit) with an associated mesh but the chunk itself isn't loaded. If a chunk has an ECS entity, it MUST have a permit for that entity, and if a chunk has a permit, then it MUST have an ECS entity. This rule should never be broken and the engine will attempt to enforce it.
