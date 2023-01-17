use std::future::Future;

use tokio::runtime::Runtime;

pub fn get_future_sync<F: Future>(future: F) -> F::Output {
    let runtime = match tokio::runtime::Handle::try_current() {
        // there's already an active Tokio runtime
        Ok(runtime) => runtime,

        // there is not already an active Tokio runtime; create one just for this call
        // todo: we could create one and store it for usage by future invocations, but let's cross
        // that bridge when we burn it.
        Err(_) => Runtime::new().unwrap().handle().to_owned(),
    };

    runtime.block_on(future)
}
