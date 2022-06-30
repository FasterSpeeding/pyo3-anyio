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

use pyo3::types::PyDict;
use pyo3::{PyAny, PyObject, PyResult, Python};

use crate::any::TaskLocals;

pub(crate) type BoxedFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

pub trait RustRuntime {
    type JoinError: std::error::Error;
    type JoinHandle: Future<Output = Result<(), Self::JoinError>> + Send;

    fn get_locals(py: Python) -> Option<TaskLocals>;
    fn spawn(fut: impl Future<Output = ()> + Send + 'static) -> Self::JoinHandle;
    fn spawn_local(fut: impl Future<Output = ()> + 'static) -> Self::JoinHandle;
    fn scope<R>(locals: TaskLocals, fut: impl Future<Output = R> + Send + 'static) -> BoxedFuture<R>;
    fn scope_local<R>(
        locals: TaskLocals,
        fut: impl Future<Output = R> + 'static,
    ) -> Pin<Box<dyn Future<Output = R> + 'static>>;
}

pub trait PyLoop: Send {
    fn call_soon(
        &self,
        py: Python,
        context: Option<&PyAny>,
        callback: &PyAny,
        args: &[PyObject], // TODO: possible impl IntoIterator<Item = T>
        kwargs: Option<&PyDict>,
    ) -> PyResult<()>;
    fn call_soon0(&self, py: Python, context: Option<&PyAny>, callback: &PyAny) -> PyResult<()> {
        self.call_soon(py, context, callback, &[], None)
    }
    fn call_soon1(&self, py: Python, context: Option<&PyAny>, callback: &PyAny, args: &[PyObject]) -> PyResult<()> {
        self.call_soon(py, context, callback, args, None)
    }
    fn await_soon(
        &self,
        py: Python,
        context: Option<&PyAny>,
        callback: &PyAny,
        args: &[PyObject],
        kwargs: Option<&PyDict>,
    ) -> PyResult<()>;
    fn await_soon0(&self, py: Python, context: Option<&PyAny>, callback: &PyAny) -> PyResult<()> {
        self.await_soon(py, context, callback, &[], None)
    }
    fn await_soon1(&self, py: Python, context: Option<&PyAny>, callback: &PyAny, args: &[PyObject]) -> PyResult<()> {
        self.await_soon(py, context, callback, args, None)
    }
    fn await_coroutine(
        &self,
        py: Python,
        context: Option<&PyAny>,
        coroutine: &PyAny,
    ) -> PyResult<BoxedFuture<PyResult<PyObject>>>;
    fn clone_box(&self) -> Box<dyn PyLoop>;
}
