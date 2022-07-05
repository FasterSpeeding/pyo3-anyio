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

//! Tokio specific functions for handling futures and coroutines.

use std::future::Future;
use std::pin::Pin;

use once_cell::sync::Lazy as LazyLock;
use pyo3::{IntoPy, PyAny, PyObject, PyResult, Python};

use crate::any::TaskLocals;
use crate::traits::BoxedFuture;

tokio::task_local! {
    static LOCALS: TaskLocals
}


// TODO: switch to std::sync::LazyLock once https://github.com/rust-lang/rust/issues/74465 is done.
static EXECUTOR: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    builder.build().expect("Failed to start executor")
});

pub struct Tokio {}  //  TODO: should this be directly public?

impl crate::traits::RustRuntime for Tokio {
    type JoinError = tokio::task::JoinError;
    type JoinHandle = tokio::task::JoinHandle<()>;

    fn spawn(fut: impl Future<Output = ()> + Send + 'static) -> Self::JoinHandle {
        EXECUTOR.spawn(fut)
    }

    fn spawn_local(fut: impl Future<Output = ()> + 'static) -> Self::JoinHandle {
        tokio::task::spawn_local(fut)
    }

    fn get_locals(py: Python) -> Option<TaskLocals> {
        LOCALS.try_with(|value| value.clone_py(py)).ok()
    }

    fn scope<R>(locals: TaskLocals, fut: impl Future<Output = R> + Send + 'static) -> BoxedFuture<R> {
        Box::pin(LOCALS.scope(locals, fut))
    }

    fn scope_local<R>(
        locals: TaskLocals,
        fut: impl Future<Output = R> + 'static,
    ) -> Pin<Box<dyn Future<Output = R> + 'static>> {
        Box::pin(LOCALS.scope(locals, fut))
    }
}

/// Convert a Rust future into a Python coroutine.
///
/// # Arguments
///
/// * `py` - The GIL token.
/// * `fut` The future to convert into a Python coroutine.
///
/// # Errors
///
/// Returns `pyo3::PyErr` if there is no running Python event loop in this
/// thread or if Anyio isn't installed in the current Python environment.
pub fn fut_into_coro<T>(py: Python, fut: impl Future<Output = PyResult<T>> + Send + 'static) -> PyResult<&PyAny>
where
    T: IntoPy<PyObject>, {
    fut_into_coro_with_locals(py, TaskLocals::default(py)?, fut)
}

/// Convert a Rust future into a Python coroutine with the passed task locals.
///
/// # Arguments
///
/// * `py` - The GIL token.
/// * `locals` - The task locals to execute the future with, if applicable.
/// * `fut` The future to convert into a Python coroutine.
///
/// # Errors
///
/// If Anyio isn't installed in the current Python environment.
pub fn fut_into_coro_with_locals<T>(
    py: Python,
    locals: TaskLocals,
    fut: impl Future<Output = PyResult<T>> + Send + 'static,
) -> PyResult<&PyAny>
where
    T: IntoPy<PyObject>, {
    crate::any::fut_into_coro::<Tokio, _>(py, locals, fut)
}

/// Convert a `!Send` Rust future into a Python coroutine.
///
/// # Arguments
///
/// * `py` - The GIL token.
/// * `fut` The future to convert into a Python coroutine.
///
/// # Errors
///
/// Returns `pyo3::PyErr` if there is no running Python event loop in this
/// thread or if Anyio isn't installed in the current Python environment.
pub fn local_fut_into_coro<T>(py: Python, fut: impl Future<Output = PyResult<T>> + 'static) -> PyResult<&PyAny>
where
    T: IntoPy<PyObject>, {
    local_fut_into_coro_with_locals(py, TaskLocals::default(py)?, fut)
}

/// Convert a `!Send` Rust future into a Python coroutine with the passed task
/// locals.
///
/// # Arguments
///
/// * `py` - The GIL token.
/// * `locals` - The task locals to execute the future with, if applicable.
/// * `fut` The future to convert into a Python coroutine.
///
/// # Errors
///
/// If Anyio isn't installed in the current Python environment.
pub fn local_fut_into_coro_with_locals<T>(
    py: Python,
    locals: TaskLocals,
    fut: impl Future<Output = PyResult<T>> + 'static,
) -> PyResult<&PyAny>
where
    T: IntoPy<PyObject>, {
    crate::any::local_fut_into_coro::<Tokio, _>(py, locals, fut)
}

/// Convert a Python coroutine to a Rust future.
///
/// # Arguments
///
/// * `coroutine` - The coroutine convert.
///
/// # Errors
///
/// Returns a `pyo3::PyErr` if this failed to schedule the callback.
///
/// The inner value of this will be a `pyo3::exceptions::PyRuntimeError` if
/// the loop isn't active.
pub fn coro_to_fut(coroutine: &PyAny) -> PyResult<impl Future<Output = PyResult<PyObject>> + Send + 'static> {
    crate::any::coro_to_fut::<Tokio>(coroutine)
}

/// Run the given future in an asynchronous Python event loop.
///
/// # Arguments
///
/// * `py` - The GIL token.
/// * `fut` - The future to run in an asynchronous Python event loop.
/// * `backend` - The Python async backend to run this in. This may be either
///   "asyncio" or "trio".
///
/// # Errors
///
/// Returns a `pyo3::PyError` if this failed to start the event loop.
///
/// This may indicate that an invalid value was passed for `backend` or that an
/// event loop is already active in the current thread.
pub fn run<T>(py: Python, fut: impl Future<Output = PyResult<T>> + Send + 'static, backend: &str) -> PyResult<T>
where
    T: Send + Sync + 'static, {
    crate::any::run::<Tokio, T>(py, fut, backend)
}
