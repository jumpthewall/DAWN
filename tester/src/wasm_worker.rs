use anyhow::{anyhow, Result};
use std::sync::mpsc;
use std::thread;
use tokio::sync::oneshot;
use wasmtime::{Engine, Instance, Linker, Module, Store, TypedFunc};

/// Request sent to the WASM worker thread
struct TransformRequest {
    input: Vec<u8>,
    response_tx: oneshot::Sender<Result<Vec<u8>>>,
}

/// Handle for communicating with the WASM worker thread
pub struct WasmWorker {
    request_tx: mpsc::Sender<TransformRequest>,
}

impl WasmWorker {
    /// Create a new WASM worker that loads the given plugin
    pub fn new(plugin_path: &str) -> Result<Self> {
        let (request_tx, request_rx) = mpsc::channel::<TransformRequest>();
        let plugin_path = plugin_path.to_string();

        thread::spawn(move || {
            if let Err(e) = run_worker(&plugin_path, request_rx) {
                eprintln!("WASM worker error: {}", e);
            }
        });

        Ok(Self { request_tx })
    }

    /// Transform a DNS packet using the WASM plugin
    pub async fn transform(&self, input: &[u8]) -> Result<Vec<u8>> {
        let (response_tx, response_rx) = oneshot::channel();

        self.request_tx
            .send(TransformRequest {
                input: input.to_vec(),
                response_tx,
            })
            .map_err(|_| anyhow!("WASM worker channel closed"))?;

        response_rx
            .await
            .map_err(|_| anyhow!("WASM worker response channel closed"))?
    }
}

/// Run the WASM worker thread
fn run_worker(plugin_path: &str, request_rx: mpsc::Receiver<TransformRequest>) -> Result<()> {
    let engine = Engine::default();
    let module = Module::from_file(&engine, plugin_path)?;
    let linker = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &module)?;

    // Get exported functions
    let alloc: TypedFunc<u32, u32> = instance
        .get_typed_func(&mut store, "alloc")
        .map_err(|e| anyhow!("Failed to get 'alloc' export: {}", e))?;

    let dealloc: TypedFunc<(u32, u32), ()> = instance
        .get_typed_func(&mut store, "dealloc")
        .map_err(|e| anyhow!("Failed to get 'dealloc' export: {}", e))?;

    let transform: TypedFunc<(u32, u32, u32, u32), u32> = instance
        .get_typed_func(&mut store, "transform")
        .map_err(|e| anyhow!("Failed to get 'transform' export: {}", e))?;

    // Process requests
    for request in request_rx {
        let result = process_transform(
            &mut store,
            &instance,
            &alloc,
            &dealloc,
            &transform,
            &request.input,
        );
        let _ = request.response_tx.send(result);
    }

    Ok(())
}

/// Process a single transform request
fn process_transform(
    store: &mut Store<()>,
    instance: &Instance,
    alloc: &TypedFunc<u32, u32>,
    dealloc: &TypedFunc<(u32, u32), ()>,
    transform: &TypedFunc<(u32, u32, u32, u32), u32>,
    input: &[u8],
) -> Result<Vec<u8>> {
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
    let output_len = transform.call(&mut *store, (input_ptr, input_len, output_ptr, output_capacity))?;

    // Read output from WASM memory
    let mut output = vec![0u8; output_len as usize];
    memory.read(&store, output_ptr as usize, &mut output)?;

    // Deallocate buffers
    dealloc.call(&mut *store, (input_ptr, input_len))?;
    dealloc.call(&mut *store, (output_ptr, output_capacity))?;

    Ok(output)
}
