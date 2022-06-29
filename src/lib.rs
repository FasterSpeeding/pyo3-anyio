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

use pyo3::types::{PyDict, PyTuple};
use pyo3::{IntoPy, PyObject, PyResult, Python};

mod any;
mod asyncio;
pub mod tokio;
pub mod traits;
mod trio;

#[pyo3::pyclass]
pub(crate) struct WrapCall {
    callback: PyObject,
}

impl WrapCall {
    fn py(py: Python, callback: PyObject) -> PyObject {
        Self { callback }.into_py(py)
    }
}

#[pyo3::pymethods]
impl WrapCall {
    #[args(callback, args, kwargs = "None")]
    fn __call__(&self, py: Python, args: &PyTuple, kwargs: Option<&PyDict>) -> PyResult<PyObject> {
        self.callback.call(py, args, kwargs)
    }
}
