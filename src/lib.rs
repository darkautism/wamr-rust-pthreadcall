#![feature(fn_traits)]

use std::any::Any;
use std::boxed::Box;
use std::ffi::c_void;
use wamr_rust_sdk::RuntimeError;
use wamr_rust_sdk::function::Function;
use wamr_rust_sdk::instance::Instance;
use wamr_rust_sdk::value::WasmValue;

/// A trait that extends the `Function` type to enable calling WebAssembly functions in a pthread context.
///
/// This trait is particularly useful for embedded systems like the ESP32, where executing WebAssembly
/// functions in a proper pthread context can prevent crashes related to threading issues. It automates
/// the creation, execution, and cleanup of a pthread, making it easier to integrate with the WebAssembly
/// Micro Runtime (WAMR) on such platforms.
///
/// # Examples
/// ```rust
/// use wamr_rust_pthreadcall::PThreadExtension;
/// use wamr_rust_sdk::function::Function;
/// use wamr_rust_sdk::instance::Instance;
/// use wamr_rust_sdk::value::WasmValue;
///
/// let params: Vec<WasmValue> = vec![];
/// let result = function
/// .call_pthread(&instance, &params)
/// .expect("Failed to call WAMR function in pthread context");
/// ```
pub trait PThreadExtension<'instance> {
    fn call_pthread(
        &self,
        instance: &'instance Instance<'instance>,
        params: &Vec<WasmValue>,
    ) -> Result<Vec<WasmValue>, RuntimeError>;
}

struct FunctionCaller<'instance> {
    function: &'instance Function<'instance>,
    instance: &'instance Instance<'instance>,
    params: &'instance Vec<WasmValue>,
}

impl<'instance> FunctionCaller<'instance> {
    fn call(&self) -> Result<Vec<WasmValue>, RuntimeError> {
        self.function.call(self.instance, self.params)
    }
}

unsafe extern "C" fn raw_fncaller(mut _arg: *mut std::ffi::c_void) -> *mut std::ffi::c_void {
    let fncaller = unsafe { Box::from_raw(_arg as *mut FunctionCaller) };
    let ret: Box<Result<Vec<WasmValue>, RuntimeError>> = Box::new(fncaller.call());
    Box::into_raw(ret) as *mut std::ffi::c_void
}

impl<'instance> PThreadExtension<'instance> for Function<'instance> {
    /// Calls a WebAssembly function in a pthread context and returns the execution result.
    ///
    /// This method creates a new pthread to execute the WebAssembly function, ensuring it runs in a
    /// proper threading context. It is especially useful on platforms like the ESP32, where direct
    /// execution of WAMR functions outside a pthread context may lead to runtime issues.
    ///
    /// # Parameters
    /// - `instance`: A reference to the `Instance` object representing the WAMR execution instance.
    /// - `params`: A reference to a vector of `WasmValue` representing the function's input parameters.
    ///
    /// # Returns
    /// - `Ok(Vec<WasmValue>)` on successful execution, containing the function's return values.
    /// - `Err(RuntimeError)` if thread creation, execution, or joining fails.
    ///
    /// # Errors
    /// This method returns a `RuntimeError::ExecutionError` if:
    /// - The pthread cannot be created (e.g., due to resource limitations).
    /// - The pthread cannot be joined after execution (e.g., due to a system error).
    ///
    /// # Notes
    /// - The pthread is created with a default stack size of 4096 bytes. This can be sufficient for
    /// most simple WebAssembly functions but may need adjustment for more complex workloads on
    /// resource-constrained systems like the ESP32.
    /// - The method handles thread creation and cleanup automatically, using the ESP-IDF pthread API.
    /// - This is designed to work seamlessly with the `wamr-rust-sdk` and is particularly beneficial
    /// for avoiding pthread-related crashes on embedded platforms.
    ///
    /// # Examples
    /// ```rust
    /// use wamr_rust_pthreadcall::PThreadExtension;
    /// use wamr_rust_sdk::function::Function;
    /// use wamr_rust_sdk::instance::Instance;
    /// use wamr_rust_sdk::value::WasmValue;
    ///
    /// let params: Vec<WasmValue> = vec![];
    /// match function.call_pthread(&instance, &params) {
    /// Ok(results) => println!("Function returned: {:?}", results),
    /// Err(e) => eprintln!("Error calling function: {:?}", e),
    /// }
    /// ```
    fn call_pthread(
        &self,
        instance: &'instance Instance<'instance>,
        params: &Vec<WasmValue>,
    ) -> Result<Vec<WasmValue>, RuntimeError> {
        let mut thread: u32 = 0;

        let mut attr = esp_idf_svc::sys::pthread_attr_t::default();
        attr.stacksize = 4096;
        attr.detachstate = esp_idf_svc::sys::PTHREAD_CREATE_JOINABLE as i32;

        let fncaller = FunctionCaller {
            function: &self,
            instance,
            params,
        };
        let ptr_fncaller = Box::into_raw(Box::new(fncaller)) as *mut std::ffi::c_void;

        let res = unsafe {
            esp_idf_svc::sys::pthread_create(&mut thread, &attr, Some(raw_fncaller), ptr_fncaller)
        };

        if res != 0 {
            return Err(RuntimeError::ExecutionError(wamr_rust_sdk::ExecError {
                message: format!("Failed to create thread: {}", res),
                exit_code: res.abs() as u32,
            }));
        } else {
            unsafe {
                let mut raw_ret = std::ptr::null_mut();
                let join_ret = esp_idf_svc::sys::pthread_join(thread as _, &mut raw_ret);
                if join_ret != 0 {
                    return Err(RuntimeError::ExecutionError(wamr_rust_sdk::ExecError {
                        message: format!("Failed to join thread: {}", join_ret),
                        exit_code: join_ret.abs() as u32,
                    }));
                }

                let instance_data =
                    Box::from_raw(raw_ret as *mut Result<Vec<WasmValue>, RuntimeError>);
                return *instance_data;
            }
        }
    }
}

// 定義一個結構來包裝胖指標
struct ClosureWrapper {
    closure: Box<dyn FnOnce() -> Box<dyn Any + Send> + Send>,
}

// pthread 回調函數
unsafe extern "C" fn raw_closurescaller(arg: *mut c_void) -> *mut c_void {
    let wrapper = unsafe { Box::from_raw(arg as *mut ClosureWrapper) };
    let result: Box<dyn Any + Send> = (wrapper.closure)(); // 呼叫閉包
    Box::into_raw(result) as *mut c_void
}

/// Executes a closure in a pthread context and returns the result of the closure's execution.
///
/// This function provides a convenient wrapper for running operations that require a pthread context,
/// such as modifying runtime parameters or registering host functions in the WebAssembly Micro
/// Runtime (WAMR). It is especially useful on embedded platforms like the ESP32, where direct
/// execution outside a pthread context may cause runtime crashes. The function handles the creation,
/// execution, and cleanup of a pthread, simplifying integration with WAMR and other pthread-dependent
/// components.
///
/// # Parameters
/// - `stacksize`: The stack size for the pthread in bytes. This value should be chosen based on the
///   closure's requirements and the target system's constraints. A typical default is 4096 bytes,
///   but complex operations may require a larger stack.
/// - `f`: A closure that performs the desired operations and returns a value of type `T`. The closure
///   must implement `FnOnce() -> T`, `Send`, and have a static lifetime (`'static`).
///
/// # Returns
/// - `Ok(T)` if the closure executes successfully, where `T` is the return type of the closure.
/// - `Err(RuntimeError)` if there is an error in creating, executing, or joining the pthread, or if
///   there is a type mismatch in the result.
///
/// # Errors
/// This function returns a `RuntimeError::ExecutionError` if:
/// - The pthread cannot be created (e.g., due to resource limitations).
/// - The pthread cannot be joined after execution (e.g., due to a system error).
/// - There is a type mismatch when downcasting the closure's result.
///
/// # Notes
/// - The function leverages the ESP-IDF pthread API to ensure compatibility with the ESP32's RTOS
///   environment.
/// - The closure is executed in a separate thread, and its result is safely transferred back to the
///   calling thread using `Box` and `Any` for dynamic typing.
/// - This wrapper is particularly valuable when initializing WAMR components, such as registering
///   host functions or creating instances, which may require a pthread context on certain platforms.
///
/// # Examples
/// ```rust
/// use wamr_rust_pthreadcall::call_pthread;
/// use wamr_rust_sdk::RuntimeError;
/// use wamr_rust_sdk::value::WasmValue;
/// use wamr_rust_sdk::{Runtime, Module, Instance, Function};
///
/// let result: Result<Vec<WasmValue>, RuntimeError> = call_pthread(4096, || {
///     let runtime = Runtime::builder()
///         .use_system_allocator()
///         .register_host_function("vTaskDelay", esp_idf_svc::sys::vTaskDelay as *mut _)
///         .build()
///         .unwrap();
///     let module = Module::from_file(&runtime, "example.wasm").unwrap();
///     let instance = Instance::new(&runtime, &module, 1024).unwrap();
///     let function = Function::find_export_func(&instance, "main").unwrap();
///     let params: Vec<WasmValue> = vec![];
///     function.call(&instance, &params)
/// });
///
/// match result {
///     Ok(values) => println!("Function returned: {:?}", values),
///     Err(e) => eprintln!("Error executing closure: {:?}", e),
/// }
/// ```
///
/// In this example, the closure initializes a WAMR runtime, loads a module, creates an instance,
/// and calls an exported function, all within a pthread context with a stack size of 4096 bytes.
/// The result is returned as a `Vec<WasmValue>` wrapped in a `Result`.
pub fn call_pthread<F, T>(stacksize: i32, f: F) -> Result<T, RuntimeError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let mut thread: u32 = 0;
    let mut attr = esp_idf_svc::sys::pthread_attr_t::default();
    attr.stacksize = 4096;
    attr.detachstate = esp_idf_svc::sys::PTHREAD_CREATE_JOINABLE as i32;

    let wrapped_f = move || {
        let result = f();
        Box::new(result) as Box<dyn Any + Send>
    };

    let wrapper = ClosureWrapper {
        closure: Box::new(wrapped_f),
    };
    let ptr = Box::into_raw(Box::new(wrapper)) as *mut c_void;

    let res = unsafe {
        esp_idf_svc::sys::pthread_create(&mut thread, &attr, Some(raw_closurescaller), ptr)
    };

    if res != 0 {
        return Err(RuntimeError::ExecutionError(wamr_rust_sdk::ExecError {
            message: format!("Failed to create thread: {}", res),
            exit_code: res.abs() as u32,
        }));
    } else {
        unsafe {
            let mut raw_ret = std::ptr::null_mut();
            let join_ret = esp_idf_svc::sys::pthread_join(thread as _, &mut raw_ret);
            if join_ret != 0 {
                return Err(RuntimeError::ExecutionError(wamr_rust_sdk::ExecError {
                    message: format!("Failed to join thread: {}", join_ret),
                    exit_code: join_ret.abs() as u32,
                }));
            }

            let boxed_result: Box<dyn Any + Send> = Box::from_raw(raw_ret as *mut _);
            let result_boxed_t: Box<T> = boxed_result.downcast::<T>().map_err(|_| {
                RuntimeError::ExecutionError(wamr_rust_sdk::ExecError {
                    message: "Type mismatch in thread result".to_string(),
                    exit_code: 1,
                })
            })?;
            Ok(*result_boxed_t)
        }
    }
}
