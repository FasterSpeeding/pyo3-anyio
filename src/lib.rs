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

//! `PyO3` utility bindings for Anyio's event loop.

#![deny(clippy::pedantic)]
#![allow(clippy::borrow_deref_ref)] // Leads to a ton of false positives around args of py types.
#![allow(clippy::missing_panics_doc)] // TODO: finalise and document the panics
#![allow(clippy::used_underscore_binding)] // Doesn't work with macros
#![warn(missing_docs)]

use once_cell::sync::OnceCell as OnceLock;
use pyo3::exceptions::PyRuntimeError;
use pyo3::types::{PyDict, PyTuple};
use pyo3::{IntoPy, PyAny, PyObject, PyResult, Python, ToPyObject};
pub mod any;
mod asyncio;
pub mod tokio;
pub mod traits;
mod trio;

pub use crate::asyncio::Asyncio;
use crate::traits::PyLoop;
pub use crate::trio::Trio;


static NONE: OnceLock<PyObject> = OnceLock::new();
// TODO: switch to std::sync::OnceLock once https://github.com/rust-lang/rust/issues/74465 is done.
static SYS_MODULES: OnceLock<PyObject> = OnceLock::new();


pub(crate) fn import_none(py: Python) -> &PyAny {
    NONE.get_or_init(|| py.None()).as_ref(py)
}

fn import_sys_modules(py: Python) -> PyResult<&PyAny> {
    SYS_MODULES
        .get_or_try_init(|| Ok(py.import("sys")?.getattr("modules")?.to_object(py)))
        .map(|value| value.as_ref(py))
}

#[pyo3::pyclass]
pub(crate) struct ContextWrap {
    callback: PyObject,
    context: Option<PyObject>,
}

impl ContextWrap {
    fn py(context: Option<&PyAny>, callback: &PyAny) -> PyObject {
        let py = callback.py();
        Self {
            callback: callback.to_object(py),
            context: context.map(|value| value.to_object(py)),
        }
        .into_py(py)
    }
}

#[pyo3::pymethods]
impl ContextWrap {
    #[args(callback, args, kwargs = "None")]
    fn __call__(&self, py: Python, mut args: Vec<PyObject>, kwargs: Option<&PyDict>) -> PyResult<PyObject> {
        if let Some(context) = self.context.as_ref() {
            args.insert(0, self.callback.clone_ref(py));
            context.call_method(py, "run", PyTuple::new(py, args), kwargs)
        } else {
            self.callback.call(py, PyTuple::new(py, args), kwargs)
        }
    }
}

/// Get the current thread's active event loop.
///
/// # Errors
///
/// This will return a `Err(pyo3::PyErr)` where the inner type is
/// `pyo3::exceptions::PyRuntimeError` if there is no running event loop.
pub fn get_running_loop(py: Python) -> PyResult<Box<dyn PyLoop>> {
    // sys.modules is used here to avoid unnecessarily trying to import asyncio or
    // Trio if it hasn't been imported yet or isn't installed.
    let modules = import_sys_modules(py)?;

    // TODO: switch to && let Some(loop) = ... once https://github.com/rust-lang/rust/pull/94927
    // is merged and released
    if modules.contains("asyncio")? {
        if let Some(loop_) = Asyncio::get_running_loop(py)? {
            return Ok(Box::new(loop_));
        }
    };

    if modules.contains("trio")? {
        if let Some(loop_) = Trio::get_running_loop(py)? {
            return Ok(Box::new(loop_));
        }
    };

    Err(PyRuntimeError::new_err("No running event loop"))
}
