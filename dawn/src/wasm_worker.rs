//! WASM plugin execution engine.
//!
//! Provides the [`WasmWorker`] type for loading and executing WASM transform plugins.

use anyhow::{anyhow, Result};
use wasmtime::{Engine, Instance, InstancePre, Linker, Module, Store};

/// WASM worker using InstancePre for efficient parallel instantiation.
///
/// Pre-compiles the WASM module at construction time, allowing fast parallel
/// instantiation for concurrent request handling.
pub struct WasmWorker {
    engine: Engine,
    instance_pre: InstancePre<()>,
}

impl WasmWorker {
    /// Create a new WASM worker that loads the given plugin
    pub fn new(plugin_path: &str) -> Result<Self> {
        let engine = Engine::default();
        let module = Module::from_file(&engine, plugin_path)?;
        let linker = Linker::new(&engine);
        let instance_pre = linker.instantiate_pre(&module)?;

        Ok(Self {
            engine,
            instance_pre,
        })
    }

    /// Transform a DNS packet using the WASM plugin
    pub async fn transform(&self, input: &[u8]) -> Result<Vec<u8>> {
        let engine = self.engine.clone();
        let instance_pre = self.instance_pre.clone();
        let input = input.to_vec();

        tokio::task::spawn_blocking(move || {
            let mut store = Store::new(&engine, ());
            let instance = instance_pre.instantiate(&mut store)?;
            process_transform(&mut store, &instance, &input)
        })
        .await?
    }
}

/// Processes a single transform request within a WASM instance.
///
/// Allocates input/output buffers in WASM memory, calls the plugin's transform
/// function, and returns the transformed packet.
fn process_transform(store: &mut Store<()>, instance: &Instance, input: &[u8]) -> Result<Vec<u8>> {
    let alloc = instance
        .get_typed_func::<u32, u32>(&mut *store, "alloc")
        .map_err(|e| anyhow!("Failed to get 'alloc' export: {}", e))?;

    let dealloc = instance
        .get_typed_func::<(u32, u32), ()>(&mut *store, "dealloc")
        .map_err(|e| anyhow!("Failed to get 'dealloc' export: {}", e))?;

    let transform = instance
        .get_typed_func::<(u32, u32, u32, u32), u32>(&mut *store, "transform")
        .map_err(|e| anyhow!("Failed to get 'transform' export: {}", e))?;

    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| anyhow!("Failed to get WASM memory"))?;

    let input_len = input.len() as u32;
    // Output buffer: allow some growth for duplicated questions
    let output_capacity = (input.len() + 256) as u32;

    // Allocate input buffer in WASM memory
    let input_ptr = alloc.call(&mut *store, input_len)?;

    // Allocate output buffer in WASM memory
    let output_ptr = alloc.call(&mut *store, output_capacity)?;

    // Write input data to WASM memory
    memory.write(&mut *store, input_ptr as usize, input)?;

    // Call transform
    let output_len = transform.call(
        &mut *store,
        (input_ptr, input_len, output_ptr, output_capacity),
    )?;

    // Read output from WASM memory
    let mut output = vec![0u8; output_len as usize];
    memory.read(&store, output_ptr as usize, &mut output)?;

    // Deallocate buffers
    dealloc.call(&mut *store, (input_ptr, input_len))?;
    dealloc.call(&mut *store, (output_ptr, output_capacity))?;

    Ok(output)
}
