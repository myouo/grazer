//! Raw WASM conformance harness; the Vec is retained in thread-local storage.
use std::cell::RefCell;
thread_local! { static TRACE: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) }; }

#[unsafe(no_mangle)]
pub extern "C" fn run_trace(ticks: u32) -> *const u64 {
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        *trace = grazer::demo::trace(ticks.min(100_000));
        trace.as_ptr()
    })
}
