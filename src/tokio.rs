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

use once_cell::sync::Lazy;
use pyo3::{IntoPy, PyAny, PyObject, PyResult, Python};

use crate::any::TaskLocals;
use crate::traits::BoxedFuture;

tokio::task_local! {
    static LOCALS: TaskLocals
}


// TODO: switch to std::sync::LazyLock once https://github.com/rust-lang/rust/issues/74465 is done.
static EXECUTOR: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    builder.build().expect("Failed to start executor")
});

struct Tokio {}

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

pub fn future_into_py<T>(py: Python, fut: impl Future<Output = PyResult<T>> + Send + 'static) -> PyResult<&PyAny>
where
    T: IntoPy<PyObject>, {
    future_into_py_with_loop(py, TaskLocals::default(py)?, fut)
}

pub fn future_into_py_with_loop<T>(
    py: Python,
    locals: TaskLocals,
    fut: impl Future<Output = PyResult<T>> + Send + 'static,
) -> PyResult<&PyAny>
where
    T: IntoPy<PyObject>, {
    crate::any::future_into_py::<Tokio, _>(py, locals, fut)
}

pub fn local_future_into_py<T>(py: Python, fut: impl Future<Output = PyResult<T>> + 'static) -> PyResult<&PyAny>
where
    T: IntoPy<PyObject>, {
    local_future_into_py_with_loop(py, TaskLocals::default(py)?, fut)
}

pub fn local_future_into_py_with_loop<T>(
    py: Python,
    locals: TaskLocals,
    fut: impl Future<Output = PyResult<T>> + 'static,
) -> PyResult<&PyAny>
where
    T: IntoPy<PyObject>, {
    crate::any::local_future_into_py::<Tokio, _>(py, locals, fut)
}

pub fn to_future(py: Python, coroutine: &PyAny) -> PyResult<impl Future<Output = PyResult<PyObject>> + Send + 'static> {
    crate::any::to_future::<Tokio>(py, coroutine)
}
