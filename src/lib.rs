use std::boxed::Box;
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
