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

//! Traits used for handling Rust async runtimes and Python event loops.

use std::future::Future;
use std::pin::Pin;

use pyo3::types::PyDict;
use pyo3::{PyAny, PyObject, PyResult, Python};

use crate::any::TaskLocals;

pub(crate) type BoxedFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Trait used to represent a Rust async runtime.
pub trait RustRuntime {
    /// Error returned by a join handle after being awaited.
    type JoinError: std::error::Error;
    /// The future of a spawned task.
    type JoinHandle: Future<Output = Result<(), Self::JoinError>> + Send;

    /// Get the current task's set locals.
    fn get_locals() -> Option<TaskLocals>;

    /// Get the current task's set locals.
    ///
    /// This should be preferred over `std::clone::Clone` if you are already
    /// holding the GIL.
    fn get_locals_py(py: Python) -> Option<TaskLocals>;

    /// Spawn a future on this runtime.
    fn spawn(fut: impl Future<Output = ()> + Send + 'static) -> Self::JoinHandle;

    /// Spawn a `!Send` future on this runtime
    fn spawn_local(fut: impl Future<Output = ()> + 'static) -> Self::JoinHandle;

    /// Scope the task locals for a future.
    fn scope<R>(locals: TaskLocals, fut: impl Future<Output = R> + Send + 'static) -> BoxedFuture<R>;

    /// Set the task locals for a `!Send` future.
    fn scope_local<R>(
        locals: TaskLocals,
        fut: impl Future<Output = R> + 'static,
    ) -> Pin<Box<dyn Future<Output = R> + 'static>>;
}

/// Reference to the running Python event loop.
pub trait PyLoop: Send + Sync {
    /// Call a Python function soon in this event loop.
    ///
    /// # Arguments
    ///
    /// * `context` - The Python `contextvar` context to call this function in,
    ///   if applicable.
    /// * `callback` - The function to call.
    /// * `args` - Slice of positional arguments to pass to the function.
    /// * `kwargs` Python dict of keyword arguments to pass to the function.
    ///
    /// # Errors
    ///
    /// Returns a `pyo3::PyErr` if this failed to schedule the callback.
    ///
    /// The inner value of this will be a `pyo3::exceptions::PyRuntimeError` if
    /// the loop isn't active.
    fn call_soon(
        &self,
        context: Option<&PyAny>,
        callback: &PyAny,
        args: &[PyObject],
        kwargs: Option<&PyDict>,
    ) -> PyResult<()>;
    /// Call a Python function soon (with no arguments) in this event loop.
    ///
    /// # Arguments
    ///
    /// * `context` - The Python `contextvar` context to call this function in,
    ///   if applicable.
    /// * `callback` - The function to call.
    ///
    /// # Errors
    ///
    /// Returns a `pyo3::PyErr` if this failed to schedule the callback.
    ///
    /// The inner value of this will be a `pyo3::exceptions::PyRuntimeError` if
    /// the loop isn't active.
    fn call_soon0(&self, context: Option<&PyAny>, callback: &PyAny) -> PyResult<()> {
        self.call_soon(context, callback, &[], None)
    }
    /// Call a Python function soon (with only positional arguments) in this
    /// event loop.
    ///
    /// # Arguments
    ///
    /// * `context` - The Python `contextvar` context to call this function in,
    ///   if applicable.
    /// * `callback` - The function to call.
    ///
    /// # Errors
    ///
    /// Returns a `pyo3::PyErr` if this failed to schedule the callback.
    ///
    /// The inner value of this will be a `pyo3::exceptions::PyRuntimeError` if
    /// the loop isn't active.
    fn call_soon1(&self, context: Option<&PyAny>, callback: &PyAny, args: &[PyObject]) -> PyResult<()> {
        self.call_soon(context, callback, args, None)
    }
    /// Call an async Python function soon in this event loop.
    ///
    /// # Arguments
    ///
    /// * `context` - The Python `contextvar` context to call this function in,
    ///   if applicable.
    /// * `callback` - The function to call.
    /// * `args` - Slice of positional arguments to pass to the function.
    /// * `kwargs` Python dict of keyword arguments to pass to the function.
    ///
    /// # Errors
    ///
    /// Returns a `pyo3::PyErr` if this failed to schedule the callback.
    /// The inner value of this will be a `pyo3::exceptions::PyRuntimeError` if
    /// the loop isn't active.
    fn call_soon_async(
        &self,
        context: Option<&PyAny>,
        callback: &PyAny,
        args: &[PyObject],
        kwargs: Option<&PyDict>,
    ) -> PyResult<()>;
    /// Call an async Python function soon (with no arguments) in this event
    /// loop.
    ///
    /// # Arguments
    ///
    /// * `context` - The Python `contextvar` context to call this function in,
    ///   if applicable.
    /// * `callback` - The function to call.
    /// * `args` - Slice of positional arguments to pass to the function.
    /// * `kwargs` Python dict of keyword arguments to pass to the function.
    ///
    /// # Errors
    ///
    /// Returns a `pyo3::PyErr` if this failed to schedule the callback.
    /// The inner value of this will be a `pyo3::exceptions::PyRuntimeError` if
    /// the loop isn't active.
    fn call_soon_async0(&self, context: Option<&PyAny>, callback: &PyAny) -> PyResult<()> {
        self.call_soon_async(context, callback, &[], None)
    }
    /// Call an async Python function soon (with only positional arguments) in
    /// this event loop.
    ///
    /// # Arguments
    ///
    /// * `context` - The Python `contextvar` context to call this function in,
    ///   if applicable.
    /// * `callback` - The function to call.
    /// * `args` - Slice of positional arguments to pass to the function.
    /// * `kwargs` Python dict of keyword arguments to pass to the function.
    ///
    /// # Errors
    ///
    /// Returns a `pyo3::PyErr` if this failed to schedule the callback.
    /// The inner value of this will be a `pyo3::exceptions::PyRuntimeError` if
    /// the loop isn't active.
    fn call_soon_async1(&self, context: Option<&PyAny>, callback: &PyAny, args: &[PyObject]) -> PyResult<()> {
        self.call_soon_async(context, callback, args, None)
    }

    /// Convert a Python coroutine to a future.
    ///
    /// This will spawn the coroutine as a task in this event loop.
    ///
    /// # Arguments
    ///
    /// * `context` - The Python `contextvar` context to await this coroutine
    ///   in, if applicable.
    /// * `coroutine` The Python coroutine to await.
    ///
    /// # Errors
    ///
    /// Returns a `pyo3::PyErr` if this failed to schedule the callback.
    ///
    /// The inner value of this will be a `pyo3::exceptions::PyRuntimeError` if
    /// the loop isn't active.
    fn coro_to_fut(&self, context: Option<&PyAny>, coroutine: &PyAny) -> PyResult<BoxedFuture<PyResult<PyObject>>>;

    #[doc(hidden)] // Internal method used to implement clone for Box<PyLoop>
    fn clone_box(&self) -> Box<dyn PyLoop>;
}

impl Clone for Box<dyn PyLoop> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
