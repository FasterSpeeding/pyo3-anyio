// BSD 3-Clause License
//
// Copyright (c) 2022, Lucina
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// * Redistributions of source code must retain the above copyright notice, this
//   list of conditions and the following disclaimer.
//
// * Redistributions in binary form must reproduce the above copyright notice,
//   this list of conditions and the following disclaimer in the documentation
//   and/or other materials provided with the distribution.
//
// * Neither the name of the copyright holder nor the names of its contributors
//   may be used to endorse or promote products derived from this software
//   without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.

//! Functions and structs used for interacting with any Rust runtime.
use std::future::Future;

use once_cell::sync::OnceCell;
use pyo3::types::PyDict;
use pyo3::{IntoPy, PyAny, PyErr, PyObject, PyResult, Python, ToPyObject};

use crate::traits::{BoxedFuture, PyLoop, RustRuntime};

// TODO: switch to std::sync::OnceLock once https://github.com/rust-lang/rust/issues/74465 is done.
static PY_ONE_SHOT: OnceCell<PyObject> = OnceCell::new();


fn py_one_shot(py: Python) -> PyResult<&PyAny> {
    PY_ONE_SHOT
        .get_or_try_init(|| {
            let globals = PyDict::new(py);
            py.run(
                r#"
import anyio
import copy

class OneShotChannel:
    __slots__ = ("_channel", "_exception", "_value")

    def __init__(self):
        self._channel = anyio.Event()
        self._exception = None
        self._value = None

    def __await__(self):
        return self.get().__await__()

    def set(self, value, /):
        if self._channel.is_set():
            raise RuntimeError("Channel already set")

        self._value = value
        self._channel.set()

    def set_exception(self, exception, /):
        if self._channel.is_set():
            raise RuntimeError("Channel already set")

        self._exception = exception
        self._channel.set()

    async def get(self):
        if not self._channel.is_set():
            await self._channel.wait()

        if self._exception:
            raise copy.copy(self._exception)

        return self._value
        "#,
                Some(globals),
                None,
            )?;

            Ok::<_, PyErr>(globals.get_item("OneShotChannel").unwrap().to_object(py))
        })?
        .as_ref(py)
        .call0()
}

/// Task locals used to track the current event loop and `contextvar` context.
#[derive(Clone)]
pub struct TaskLocals {
    py_loop: Box<dyn PyLoop>,
    context: Option<PyObject>,
}

impl TaskLocals {
    /// Create a new task locals.
    ///
    /// # Arguments
    /// * `py_loop` - The Python event loop this is bound to.
    /// * `context` - The `contextvar` context this is bound to, if applicable.
    pub fn new(py_loop: Box<dyn PyLoop>, context: Option<PyObject>) -> Self {
        Self { py_loop, context }
    }

    /// Create a new task locals from the current context.
    ///
    /// This will return a PyError if there is no running event loop in this
    /// thread.
    ///
    /// # Arguments
    /// * `py` - The GIL hold token.
    pub fn default(py: Python) -> PyResult<Self> {
        Ok(Self::new(crate::get_running_loop(py)?, None))
    }

    /// Clone this task locals.
    ///
    /// This should be preferred over `std::clone::Clone` if you are already
    /// holding the GIL.
    ///
    /// # Arguments
    /// * `py` - The GIL hold token.
    pub fn clone_py(&self, py: Python) -> Self {
        Self {
            py_loop: self.py_loop.clone(),
            context: self.context.as_ref().map(|value| value.clone_ref(py)),
        }
    }

    fn _context_ref<'a>(&'a self, py: Python<'a>) -> Option<&'a PyAny> {
        self.context.as_ref().map(|value| value.as_ref(py))
    }

    /// Call a python function soon in this event loop.
    ///
    /// # Arguments
    /// * `py` - The GIL hold token.
    /// * `callback` - The function to call.
    /// * `args` - Slice of positional arguments to pass to the function.
    /// * `kwargs` Python dict of keyword arguments to pass to the function.
    pub fn call_soon(&self, py: Python, callback: &PyAny, args: &[PyObject], kwargs: Option<&PyDict>) -> PyResult<()> {
        self.py_loop
            .call_soon(py, self._context_ref(py), callback, args, kwargs)
    }

    /// Call a python function soon (with no arguments) in this event loop.
    ///
    /// # Arguments
    /// * `py` - The GIL hold token.
    /// * `callback` - The function to call.
    pub fn call_soon0(&self, py: Python, callback: &PyAny) -> PyResult<()> {
        self.call_soon(py, callback, &[], None)
    }

    /// Call a python function soon (with only positional arguments) in this
    /// event loop.
    ///
    /// # Arguments
    /// * `py` - The GIL hold token.
    /// * `callback` - The function to call.
    /// * `args` - Slice of positional arguments to pass to the function.
    pub fn call_soon1(&self, py: Python, callback: &PyAny, args: &[PyObject]) -> PyResult<()> {
        self.call_soon(py, callback, args, None)
    }

    /// Call an async python function soon in this event loop.
    ///
    /// # Arguments
    /// * `py` - The GIL hold token.
    /// * `callback` - The function to call.
    /// * `args` - Slice of positional arguments to pass to the function.
    /// * `kwargs` Python dict of keyword arguments to pass to the function.
    pub fn await_soon(&self, py: Python, callback: &PyAny, args: &[PyObject], kwargs: Option<&PyDict>) -> PyResult<()> {
        self.py_loop
            .await_soon(py, self._context_ref(py), callback, args, kwargs)
    }

    /// Call an async python function soon (with no arguments) in this event
    /// loop.
    ///
    /// # Arguments
    /// * `py` - The GIL hold token.
    /// * `callback` - The function to call.
    pub fn await_soon0(&self, py: Python, callback: &PyAny) -> PyResult<()> {
        self.await_soon(py, callback, &[], None)
    }

    /// Call an async python function soon (with only positional arguments
    /// arguments) in this event loop.
    ///
    /// # Arguments
    /// * `py` - The GIL hold token.
    /// * `callback` - The function to call.
    /// * `args` - Slice of positional arguments to pass to the function.
    pub fn await_soon1(&self, py: Python, callback: &PyAny, args: &[PyObject]) -> PyResult<()> {
        self.await_soon(py, callback, args, None)
    }

    /// Convert a Python coroutine to a Rust future.
    ///
    /// This will spawn the coroutine as a task in this event loop.
    ///
    /// # Arguments
    /// * `py` - The GIL hold token.
    /// * `coroutine` The Python coroutine to await.
    pub fn to_future(&self, py: Python, coroutine: &PyAny) -> PyResult<BoxedFuture<PyResult<PyObject>>> {
        self.py_loop.to_future(py, self._context_ref(py), coroutine)
    }
}

/// Convert a !Send Rust future into a Python coroutine.
///
/// # Arguments
/// * `py` - The GIL hold token.
/// * `locals` - The task locals to execute the future with, if applicable.
/// * `fut` The future to convert into a Python coroutine.
pub fn local_future_into_py<R, T>(
    py: Python,
    locals: TaskLocals,
    fut: impl Future<Output = PyResult<T>> + 'static,
) -> PyResult<&PyAny>
where
    R: RustRuntime,
    T: IntoPy<PyObject>, {
    let channel = py_one_shot(py)?;
    let set_value = channel.getattr("set")?.to_object(py);
    let set_exception = channel.getattr("set_exception")?.to_object(py);

    R::spawn_local(R::scope_local(locals, async move {
        let result = fut.await;
        Python::with_gil(|py| {
            let locals = R::get_locals(py).unwrap();
            match result {
                Ok(value) => locals
                    .call_soon1(py, set_value.as_ref(py), &[value.into_py(py)])
                    .unwrap(),
                Err(err) => locals
                    .call_soon1(py, set_exception.as_ref(py), &[err.to_object(py)])
                    .unwrap(),
            };
        });
    }));

    Ok(channel)
}

/// Convert a Rust future into a Python coroutine.
///
/// # Arguments
/// * `py` - The GIL hold token.
/// * `locals` - The task locals to execute the future with, if applicable.
/// * `fut` The future to convert into a Python coroutine.
pub fn future_into_py<R, T>(
    py: Python,
    locals: TaskLocals,
    fut: impl Future<Output = PyResult<T>> + Send + 'static,
) -> PyResult<&PyAny>
where
    R: RustRuntime,
    T: IntoPy<PyObject>, {
    let channel = py_one_shot(py)?;
    let set_value = channel.getattr("set")?.to_object(py);
    let set_exception = channel.getattr("set_exception")?.to_object(py);

    R::spawn(R::scope(locals, async move {
        let result = fut.await;
        Python::with_gil(|py| {
            let locals = R::get_locals(py).unwrap();
            match result {
                Ok(value) => locals
                    .call_soon1(py, set_value.as_ref(py), &[value.into_py(py)])
                    .unwrap(),
                Err(err) => locals
                    .call_soon1(py, set_exception.as_ref(py), &[err.to_object(py)])
                    .unwrap(),
            };
        });
    }));

    Ok(channel)
}

/// Convert a Python coroutine to a Rust future.
///
/// # Arguments
/// * `py` - The GIL hold token.
/// * `coroutine` - The coroutine convert.
pub fn to_future<R: RustRuntime>(
    py: Python,
    coroutine: &PyAny,
) -> PyResult<impl Future<Output = PyResult<PyObject>> + Send + 'static> {
    // TODO: handling when None
    R::get_locals(py).unwrap().to_future(py, coroutine)
}
