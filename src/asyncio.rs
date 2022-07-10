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
use once_cell::sync::OnceCell as OnceLock;
use pyo3::conversion::AsPyPointer;
use pyo3::exceptions::PyRuntimeError;
use pyo3::types::{PyDict, PyTuple};
use pyo3::{IntoPy, Py, PyAny, PyObject, PyResult, Python, ToPyObject};

use crate::traits::{BoxedFuture, PyLoop};
use crate::ContextWrap;

// TODO: switch to std::sync::OnceLock once https://github.com/rust-lang/rust/issues/74465 is done.
static ASYNCIO: OnceLock<PyObject> = OnceLock::new();
static NONE: OnceLock<PyObject> = OnceLock::new();

fn import_asyncio(py: Python) -> PyResult<&PyAny> {
    ASYNCIO
        .get_or_try_init(|| Ok(py.import("asyncio")?.to_object(py)))
        .map(|value| value.as_ref(py))
}

fn import_none(py: Python) -> &PyAny {
    NONE.get_or_init(|| py.None()).as_ref(py)
}


#[pyo3::pyclass]
struct AsyncioHook {
    sender: async_oneshot::Sender<PyResult<PyObject>>,
}

#[pyo3::pymethods]
impl AsyncioHook {
    fn callback(&mut self, py: Python, task: &PyAny) {
        let result = task.call_method0("result");
        unsafe { pyo3::ffi::Py_DecRef(task.as_ptr()) }
        match result {
            Ok(result) => self.sender.send(Ok(result.to_object(py))).unwrap(),
            Err(err) => self.sender.send(Err(err)).unwrap(),
        }
    }
}

#[pyo3::pyclass]
struct CreateEvent {
    context: Option<PyObject>,
    args: Option<Vec<PyObject>>,
    kwargs: Option<Py<PyDict>>,
}

impl CreateEvent {
    fn py(py: Python, context: Option<&PyAny>, args: Option<&[&PyAny]>, kwargs: Option<&PyDict>) -> PyObject {
        Self {
            context: context.map(|value| value.to_object(py)),
            // TODO: move away from this iter map call.
            args: args.map(|value| value.iter().map(|value| value.to_object(py)).collect()),
            kwargs: kwargs.map(|value| value.into_py(py)),
        }
        .into_py(py)
    }
}

#[pyo3::pymethods]
impl CreateEvent {
    fn __call__(
        &mut self,
        py: Python,
        event_loop: &PyAny,
        mut callback_or_coro: PyObject,
        one_shot: &PyAny,
    ) -> PyResult<()> {
        // If args is Some then `callback_or_coro` is a callback, otherwise its a
        // coroutine.
        let context = self.context.take();
        if let Some(mut args) = self.args.take() {
            let kwargs = self.kwargs.take().map(|value| value.into_ref(py));

            if let Some(ref context) = context {
                args.insert(0, callback_or_coro);
                callback_or_coro = context.call_method(py, "run", PyTuple::new(py, args), kwargs)?;
            } else {
                callback_or_coro = callback_or_coro.call(py, PyTuple::new(py, args), kwargs)?;
            };
        }

        let task = if let Some(context) = context {
            context.call_method1(py, "run", (event_loop.getattr("create_task")?, callback_or_coro))?
        } else {
            event_loop
                .call_method1("create_task", (callback_or_coro,))?
                .to_object(py)
        };

        task.call_method1(py, "add_done_callback", (one_shot,))?;

        unsafe { pyo3::ffi::Py_IncRef(task.as_ptr()) }

        Ok(())
    }
}


/// Reference to the current Asyncio event loop.
#[derive(Clone)]
pub struct Asyncio {
    event_loop: PyObject,
}


impl Asyncio {
    /// Get the current Asyncio event loop, if this is in an active loop.
    ///
    /// # Errors
    ///
    /// Returns a `pyo3::PyErr` if this failed to get the current loop.
    ///
    /// This likely indicates that an incompatibility or issue with the
    /// current Asyncio install.
    pub fn get_running_loop(py: Python) -> PyResult<Option<Self>> {
        match import_asyncio(py)?.call_method0("get_running_loop") {
            Ok(event_loop) => Ok(Some(Self {
                event_loop: event_loop.to_object(py),
            })),
            Err(err) if err.is_instance_of::<PyRuntimeError>(py) => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub(self) fn _coro_to_fut(
        &self,
        context: Option<&PyAny>,
        callback_or_coro: &PyAny,
        args: Option<&[&PyAny]>,
        kwargs: Option<&PyDict>,
    ) -> PyResult<BoxedFuture<PyResult<PyObject>>> {
        let (sender, receiver) = async_oneshot::oneshot::<PyResult<PyObject>>();
        let py = callback_or_coro.py();
        let one_shot = AsyncioHook { sender }.into_py(py);

        self.call_soon(
            None,
            CreateEvent::py(py, context, args, kwargs).as_ref(py),
            &[
                self.event_loop.as_ref(py),
                callback_or_coro,
                one_shot.getattr(py, "callback").unwrap().as_ref(py),
            ],
            None,
        )?;

        Ok(Box::pin(async move { receiver.await.unwrap() }))
    }
}

impl PyLoop for Asyncio {
    fn await_py(
        &self,
        context: Option<&PyAny>,
        callback: &PyAny,
        args: &[&PyAny],
        kwargs: Option<&PyDict>,
    ) -> PyResult<BoxedFuture<PyResult<PyObject>>> {
        self._coro_to_fut(context, callback, Some(args), kwargs)
    }

    fn call_soon(
        &self,
        context: Option<&PyAny>,
        callback: &PyAny,
        args: &[&PyAny],
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        let py = callback.py();
        if !self.event_loop.call_method0(py, "is_running")?.is_true(py)? {
            return Err(PyRuntimeError::new_err("Event loop isn't active"));
        };

        self.event_loop.call_method1(
            callback.py(),
            "call_soon_threadsafe",
            (ContextWrap::py(context, callback), PyTuple::new(py, args), kwargs),
        )?;
        Ok(())
    }

    fn call_soon_async(
        &self,
        context: Option<&PyAny>,
        callback: &PyAny,
        args: &[&PyAny],
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        let py = callback.py();
        self.call_soon(
            context,
            import_asyncio(py)?.getattr("create_task")?,
            &[
                ContextWrap::py(context, callback).as_ref(py),
                PyTuple::new(py, args),
                kwargs.map_or_else(|| import_none(py), PyDict::as_ref),
            ],
            None,
        )?;
        Ok(())
    }

    fn coro_to_fut(&self, context: Option<&PyAny>, coroutine: &PyAny) -> PyResult<BoxedFuture<PyResult<PyObject>>> {
        self._coro_to_fut(context, coroutine, None, None)
    }

    fn clone_box(&self) -> Box<dyn PyLoop> {
        Box::new(self.clone())
    }
}
