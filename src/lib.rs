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
#![allow(clippy::borrow_deref_ref)] // Leads to a ton of false positives around args of py types.
// #![warn(missing_docs)]
#![feature(let_chains)]
#![feature(once_cell)]
#![feature(trait_alias)]

use std::sync::OnceLock;

use pyo3::exceptions::PyRuntimeError;
use pyo3::types::{PyDict, PyTuple};
use pyo3::{IntoPy, PyAny, PyErr, PyObject, PyResult, Python, ToPyObject};

use crate::asyncio::Asyncio;
use crate::traits::{PyLoop, RustRuntime};
use crate::trio::Trio;

mod asyncio;
pub mod tokio;
pub mod traits;
mod trio;
use std::future::Future;

static PY_ONE_SHOT: OnceLock<PyObject> = OnceLock::new();
static SYS: OnceLock<PyObject> = OnceLock::new();

fn import_sys(py: Python) -> PyResult<&PyAny> {
    SYS.get_or_try_init(|| Ok(py.import("sys")?.to_object(py)))
        .map(|value| value.as_ref(py))
}


fn py_one_shot(py: Python<'_>) -> PyResult<&PyAny> {
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


#[pyo3::pyclass]
pub(crate) struct WrapCall {}

impl WrapCall {
    fn py(py: Python) -> PyObject {
        Self {}.into_py(py)
    }
}

#[pyo3::pymethods]
impl WrapCall {
    #[args(callback, args, kwargs = "None")]
    fn __call__<'a>(&self, callback: &'a PyAny, args: &PyTuple, kwargs: Option<&PyDict>) -> PyResult<&'a PyAny> {
        callback.call(args, kwargs)
    }
}

pub(crate) fn py_loop(py: Python) -> PyResult<Box<dyn PyLoop>> {
    // sys.modules is used here to avoid unnecessarily trying to import asyncio or
    // trio if it hasn't been imported yet or isn't installed.
    let sys = import_sys(py)?.getattr("modules")?;

    if sys.contains("asyncio")? && let Some(loop_) = Asyncio::new(py)? {
        return Ok(Box::new(loop_));
    };

    if sys.contains("trio")? && let Some(loop_) = Trio::new(py)? {
        return Ok(Box::new(loop_))
    };

    Err(PyRuntimeError::new_err("No running event loop"))
}


pub fn future_into_py<R, T>(py: Python, fut: impl Future<Output = PyResult<T>> + Send + 'static) -> PyResult<&PyAny>
where
    R: RustRuntime,
    T: IntoPy<PyObject>, {
    let channel = py_one_shot(py)?;
    let set_value = channel.getattr("set")?.to_object(py);
    let set_exception = channel.getattr("set_exception")?.to_object(py);

    R::spawn(R::scope(py_loop(py)?, async move {
        let result = fut.await;
        Python::with_gil(|py| {
            let py_loop = R::get_loop().unwrap();
            match result {
                Ok(value) => py_loop
                    .call_soon1(py, set_value.as_ref(py), vec![value.into_py(py)])
                    .unwrap(),
                Err(err) => py_loop
                    .call_soon1(py, set_exception.as_ref(py), vec![err.to_object(py)])
                    .unwrap(),
            };
        });
    }));

    Ok(channel)
}

pub fn into_future<R: RustRuntime>(
    py: Python<'_>,
    coroutine: &PyAny,
) -> PyResult<impl Future<Output = PyResult<PyObject>> + Send + 'static> {
    // TODO: handling when None
    R::get_loop().unwrap().await_coroutine(py, coroutine)
}
