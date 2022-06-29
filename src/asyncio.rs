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
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;

use pyo3::conversion::AsPyPointer;
use pyo3::exceptions::PyRuntimeError;
use pyo3::types::{PyDict, PyTuple};
use pyo3::{IntoPy, PyAny, PyObject, PyResult, Python, ToPyObject};

use crate::traits::PyLoop;
use crate::WrapCall;

type BoxedFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

static ASYNCIO: OnceLock<PyObject> = OnceLock::new();

fn import_asyncio(py: Python) -> PyResult<&PyAny> {
    ASYNCIO
        .get_or_try_init(|| Ok(py.import("asyncio")?.to_object(py)))
        .map(|value| value.as_ref(py))
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
struct CreateEvent {}

#[pyo3::pymethods]
impl CreateEvent {
    fn __call__(&self, event_loop: &PyAny, awaitable: &PyAny, one_shot: &PyAny) -> PyResult<()> {
        let task = event_loop.call_method1("create_task", (awaitable,))?;

        task.call_method1("add_done_callback", (one_shot,))?;

        unsafe { pyo3::ffi::Py_IncRef(task.as_ptr()) }

        Ok(())
    }
}

#[derive(Clone)]
pub struct Asyncio {
    event_loop: PyObject,
}


impl Asyncio {
    pub fn new(py: Python) -> PyResult<Option<Self>> {
        match import_asyncio(py)?.call_method0("get_running_loop") {
            Ok(event_loop) => Ok(Some(Self {
                event_loop: event_loop.to_object(py),
            })),
            Err(err) if err.is_instance_of::<PyRuntimeError>(py) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

impl PyLoop for Asyncio {
    fn call_soon(
        &self,
        py: Python,
        callback: &PyAny,
        mut args: Vec<PyObject>,
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        args.insert(0, callback.to_object(py));
        let args = PyTuple::new(py, args);
        self.event_loop
            .call_method1(py, "call_soon_threadsafe", (WrapCall::py(py), args, kwargs))?;
        Ok(())
    }

    fn await_soon(&self, py: Python, callback: &PyAny, args: Vec<PyObject>, kwargs: Option<&PyDict>) -> PyResult<()> {
        self.call_soon1(py, import_asyncio(py)?.getattr("create_task")?, vec![
            WrapCall::py(py),
            callback.to_object(py),
            PyTuple::new(py, args).to_object(py),
            kwargs.to_object(py),
        ])?;
        Ok(())
    }

    fn await_coroutine(&self, py: Python, coroutine: &PyAny) -> PyResult<BoxedFuture<PyResult<PyObject>>> {
        let (sender, receiver) = async_oneshot::oneshot::<PyResult<PyObject>>();
        let one_shot = AsyncioHook { sender }.into_py(py);
        self.call_soon1(py, CreateEvent {}.into_py(py).as_ref(py), vec![
            self.event_loop.clone_ref(py),
            coroutine.to_object(py),
            one_shot.getattr(py, "callback")?,
        ])?;

        Ok(Box::pin(async move { receiver.await.unwrap() }))
    }

    fn clone_box(&self) -> Box<dyn PyLoop> {
        Box::new(self.clone())
    }
}
