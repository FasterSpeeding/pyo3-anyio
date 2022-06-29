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

use pyo3::exceptions::{PyBaseException, PyRuntimeError};
use pyo3::types::PyDict;
use pyo3::{IntoPy, PyAny, PyErr, PyObject, PyResult, Python, ToPyObject};

use crate::traits::PyLoop;

type BoxedFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

static TRIO_HOOK: OnceLock<PyObject> = OnceLock::new();
static TRIO_LOW: OnceLock<PyObject> = OnceLock::new();

fn import_trio_low(py: Python) -> PyResult<&PyAny> {
    TRIO_LOW
        .get_or_try_init(|| Ok(py.import("trio.lowlevel")?.to_object(py)))
        .map(|value| value.as_ref(py))
}


fn wrap_coro(py: Python<'_>) -> PyResult<&PyAny> {
    TRIO_HOOK
        .get_or_try_init(|| {
            let globals = PyDict::new(py);
            py.run(
                r#"
async def wrap_coro(coro, one_shot, /):
    try:
        result = await coro

    except BaseException as exc:
        one_shot.set_exception(exc)

    else:
        one_shot.set(result)
            "#,
                Some(globals),
                None,
            )?;

            Ok::<_, PyErr>(globals.get_item("wrap_coro").unwrap().to_object(py))
        })
        .map(|value| value.as_ref(py))
}


#[pyo3::pyclass]
struct TrioHook {
    sender: async_oneshot::Sender<PyResult<PyObject>>,
}

#[pyo3::pymethods]
impl TrioHook {
    #[args(value, "/")]
    fn set(&mut self, value: PyObject) {
        self.sender.send(Ok(value)).unwrap();
    }

    #[args(value, "/")]
    fn set_exception(&mut self, value: &PyBaseException) {
        self.sender.send(Err(PyErr::from_value(value))).unwrap();
    }
}


#[derive(Clone)]
pub struct Trio {
    token: PyObject,
}

impl Trio {
    pub fn new(py: Python) -> PyResult<Option<Self>> {
        match import_trio_low(py)?.call_method0("current_trio_token") {
            Ok(token) => Ok(Some(Self {
                token: token.to_object(py),
            })),
            Err(err) if err.is_instance_of::<PyRuntimeError>(py) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

impl PyLoop for Trio {
    fn call_soon(&self, py: Python, callback: &PyAny, args: Vec<PyObject>, kwargs: Option<&PyDict>) -> PyResult<()> {
        self.token.call_method1(
            py,
            "run_sync_soon",
            (crate::WrapCall::py(py), callback.to_object(py), args, kwargs),
        )?;
        Ok(())
    }

    fn await_soon(
        &self,
        py: Python,
        callback: &PyAny,
        mut args: Vec<PyObject>,
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        args.insert(0, callback.to_object(py));
        self.call_soon1(py, crate::WrapCall::py(py).as_ref(py), vec![
            import_trio_low(py)?.getattr("spawn_system_task")?.to_object(py),
            args.to_object(py),
            kwargs.to_object(py),
        ])?;

        Ok(())
    }

    fn await_coroutine(&self, py: Python, coroutine: &PyAny) -> PyResult<BoxedFuture<PyResult<PyObject>>> {
        let (sender, receiver) = async_oneshot::oneshot::<PyResult<PyObject>>();
        let one_shot = TrioHook { sender }.into_py(py);

        self.call_soon1(py, import_trio_low(py)?.getattr("spawn_system_task")?, vec![
            wrap_coro(py)?.to_object(py),
            coroutine.to_object(py),
            one_shot,
        ])?;

        Ok(Box::pin(async move { receiver.await.unwrap() }))
    }

    fn clone_box(&self) -> Box<dyn PyLoop> {
        Box::new(self.clone())
    }
}
