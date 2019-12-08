use crate::run::run_python_code;
use crate::PythonBlock;
use pyo3::{ffi, types::PyDict, AsPyPointer, FromPyObject, IntoPy, PyErr, PyObject, PyResult, Python, ToPyObject};

/// An execution context for Python code.
///
/// This can be used to keep all global variables and imports intact between macro invocations:
///
/// ```
/// # #![feature(proc_macro_hygiene)]
/// # use inline_python::{Context, python};
/// let c = Context::new();
///
/// c.run(python! {
///   foo = 5
/// });
///
/// c.run(python! {
///   assert foo == 5
/// });
/// ```
///
/// You may also use it to inspect global variables after the execution of the Python code,
/// or set global variables before running:
///
/// ```
/// # #![feature(proc_macro_hygiene)]
/// # use inline_python::{Context, python};
/// let c = Context::new();
///
/// c.set("x", 13);
///
/// c.run(python! {
///   foo = x + 2
/// });
///
/// assert_eq!(c.get::<i32>("foo"), 15);
/// ```
pub struct Context {
	pub(crate) globals: PyObject,
}

impl Context {
	/// Create a new context for running Python code.
	///
	/// This function temporarily acquires the GIL.
	/// If you already have the GIL, use [`Context::new_with_gil`] instead.
	///
	/// This function panics if it fails to create the context.
	/// See [`Context::new_checked`] for a version that returns a result.
	pub fn new() -> Self {
		let gil = Python::acquire_gil();
		let py = gil.python();
		match Self::new_with_gil(py) {
			Ok(x) => x,
			Err(error) => {
				error.print(py);
				panic!("failed to create python context");
			}
		}
	}

	/// Create a new context for running python code.
	///
	/// This function temporarily acquires the GIL.
	/// If you already have the GIL, use [`Context::new_with_gil`] instead.
	pub fn new_checked() -> PyResult<Self> {
		let gil = Python::acquire_gil();
		let py = gil.python();
		Self::new_with_gil(py)
	}

	/// Create a new context for running Python code.
	///
	/// You must acquire the GIL to call this function.
	pub fn new_with_gil(py: Python) -> PyResult<Self> {
		let main_mod = unsafe { ffi::PyImport_AddModule("__main__\0".as_ptr() as *const _) };
		if main_mod.is_null() {
			return Err(PyErr::fetch(py));
		};

		let globals = PyDict::new(py);
		if unsafe { ffi::PyDict_Merge(globals.as_ptr(), ffi::PyModule_GetDict(main_mod), 0) != 0 } {
			return Err(PyErr::fetch(py));
		}

		Ok(Self {
			globals: globals.into_py(py),
		})
	}

	/// Get the globals as dictionary.
	pub fn globals<'p>(&self, py: Python<'p>) -> &'p PyDict {
		unsafe { py.from_borrowed_ptr(self.globals.as_ptr()) }
	}

	/// Retrieve a global variable from the context.
	///
	/// This function temporarily acquires the GIL.
	/// If you already have the GIL, use [`Context::get_with_gil`] instead.
	pub fn get<T: for<'p> FromPyObject<'p>>(&self, name: &str) -> T {
		self.get_with_gil(Python::acquire_gil().python(), name)
	}

	/// Retrieve a global variable from the context.
	pub fn get_with_gil<'p, T: FromPyObject<'p>>(&self, py: Python<'p>, name: &str) -> T {
		match self.globals(py).get_item(name) {
			None => panic!("Python context does not contain a variable named `{}`", name),
			Some(value) => match FromPyObject::extract(value) {
				Ok(value) => value,
				Err(e) => {
					e.print(py);
					panic!("Unable to convert `{}` to `{}`", name, std::any::type_name::<T>());
				}
			},
		}
	}

	/// Set a global variable in the context.
	///
	/// This function temporarily acquires the GIL.
	/// If you already have the GIL, use [`Context::set_with_gil`] instead.
	pub fn set<T: ToPyObject>(&self, name: &str, value: T) {
		self.set_with_gil(Python::acquire_gil().python(), name, value)
	}

	/// Set a global variable in the context.
	pub fn set_with_gil<'p, T: ToPyObject>(&self, py: Python<'p>, name: &str, value: T) {
		match self.globals(py).set_item(name, value) {
			Ok(()) => (),
			Err(e) => {
				e.print(py);
				panic!("Unable to set `{}` from a `{}`", name, std::any::type_name::<T>());
			}
		}
	}

	/// Run Python code using this context.
	///
	/// This function should be called using the `python!{}` macro:
	///
	/// ```
	/// # #![feature(proc_macro_hygiene)]
	/// # use inline_python::{Context, python};
	/// let c = Context::new();
	///
	/// c.run(python!{
	///     print("Hello World")
	/// });
	/// ```
	///
	/// This function temporarily acquires the GIL.
	/// If you already have the GIL, use [`Context::run_with_gil`] instead.
	pub fn run<F: FnOnce(&PyDict)>(&self, code: PythonBlock<F>) {
		self.run_with_gil(Python::acquire_gil().python(), code);
	}

	/// Run Python code using this context.
	///
	/// This function should be called using the `python!{}` macro, just like
	/// [`Context::run`].
	pub fn run_with_gil<'p, F: FnOnce(&PyDict)>(&self, py: Python<'p>, code: PythonBlock<F>) {
		(code.set_variables)(self.globals(py));
		match run_python_code(py, self, code.bytecode) {
			Ok(_) => (),
			Err(e) => {
				e.print(py);
				panic!("python!{...} failed to execute");
			}
		}
	}
}
