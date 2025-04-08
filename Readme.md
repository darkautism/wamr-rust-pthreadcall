# Wamr-rust-pthreadcall
This library makes it easier to integrate WAMR into ESP32, allowing you to focus on application logic without dealing with underlying pthread issues.

## Background and Problem
When running WAMR on ESP32, you might encounter crashes, especially errors related to POSIX threads (pthread). Research indicates that this happens because WAMR expects to run within a pthread context. However, the main program (```app_main```) of ESP32 is actually a FreeRTOS task, not a pthread. As a result, WAMR's internal pthread functions (e.g., ```pthread_self```) fail because they cannot retrieve the current thread ID in a non-pthread context.
According to the official documentation [POSIX Threads Support - ESP32](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/pthread.html):

```
pthread_self()

        An assert will fail if this function is called from a FreeRTOS task which is not a pthread.
```
This explains why the original WAMR crashes.

## How to Avoid the Issue
The ```wamr-rust-pthreadcall``` library provides the ```call_pthread``` function, which resolves this issue by executing WAMR functions within the correct pthread context. This prevents crashes and ensures WAMR runs stably on ESP32.

## Usage
Using this library is straightforward. First, add the dependency in your ```Cargo.toml``` file:
```toml

[dependencies]
wamr-rust-pthreadcall = "0.1.0"
```

Then, in your Rust code, use ```call_pthread``` to replace the original ```call``` method. Here's an example:
```rust

use wamr_rust_pthreadcall::PThreadExtension;
use wamr_rust_sdk::function::Function;
use wamr_rust_sdk::instance::Instance;
use wamr_rust_sdk::value::WasmValue;

let params: Vec<WasmValue> = vec![];
let result = function
    .call_pthread(&instance, &params)
    .expect("Failed to call WAMR function");
log::info!("Program exited with code: {:?}", result);
```

This way, the library automatically handles the creation and management of pthreads, ensuring WebAssembly functions are executed in the correct context.
