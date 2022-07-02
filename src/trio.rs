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
use once_cell::sync::OnceCell;
use pyo3::exceptions::{PyBaseException, PyRuntimeError};
use pyo3::types::{IntoPyDict, PyDict, PyTuple};
use pyo3::{IntoPy, PyAny, PyErr, PyObject, PyResult, Python, ToPyObject};

use crate::traits::{BoxedFuture, PyLoop};
use crate::WrapCall;

// TODO: switch to std::sync::OnceLock once https://github.com/rust-lang/rust/issues/74465 is done.
static TRIO_LOW: OnceCell<PyObject> = OnceCell::new();
static WRAP_CORO: OnceCell<PyObject> = OnceCell::new();

fn import_trio_low(py: Python) -> PyResult<&PyAny> {
    TRIO_LOW
        .get_or_try_init(|| Ok(py.import("trio.lowlevel")?.to_object(py)))
        .map(|value| value.as_ref(py))
}


fn wrap_coro(py: Python) -> &PyAny {
    WRAP_CORO
        .get_or_init(|| {
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
            )
            .unwrap();

            globals.get_item("wrap_coro").unwrap().to_object(py)
        })
        .as_ref(py)
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


/// Reference to the current Trio loop.
#[derive(Clone)]
pub struct Trio {
    token: PyObject,
}

impl Trio {
    /// Get the current Trio token if this is in an active Trio loop.
    ///
    /// # Arguments
    ///
    /// * `py` - The GIL token.
    ///
    /// # Errors
    ///
    /// Returns a `pyo3::PyErr` if this failed to get the current loop.
    /// This likely indicates that an incompatibility or issue with the
    /// current Trio install.
    pub fn get_running_loop(py: Python) -> PyResult<Option<Self>> {
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
    fn call_soon(
        &self,
        context: Option<&PyAny>,
        callback: &PyAny,
        args: &[PyObject],
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        self.token.call_method1(
            callback.py(),
            "run_sync_soon",
            (
                WrapCall::py(context, callback),
                PyTuple::new(callback.py(), args),
                kwargs,
            ),
        )?;
        Ok(())
    }

    fn call_soon_async(
        &self,
        context: Option<&PyAny>,
        callback: &PyAny,
        args: &[PyObject],
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        let py = callback.py();
        let args = &[&[callback.to_object(py)], args].concat();
        let wrapped = WrapCall::py(None, import_trio_low(py)?.getattr("spawn_system_task")?);
        self.call_soon(
            None,
            wrapped.as_ref(py),
            &[PyTuple::new(py, args).to_object(py), kwargs.to_object(py)],
            Some([("context", context)].into_py_dict(py)),
        )?;

        Ok(())
    }

    fn coro_to_fut(&self, context: Option<&PyAny>, coroutine: &PyAny) -> PyResult<BoxedFuture<PyResult<PyObject>>> {
        let py = coroutine.py();
        let (sender, receiver) = async_oneshot::oneshot::<PyResult<PyObject>>();
        let one_shot = TrioHook { sender }.into_py(py);

        self.call_soon(
            None,
            import_trio_low(py)?.getattr("spawn_system_task")?,
            &[wrap_coro(py).to_object(py), coroutine.to_object(py), one_shot],
            Some([("context", context)].into_py_dict(py)),
        )?;

        Ok(Box::pin(async move { receiver.await.unwrap() }))
    }

    fn clone_box(&self) -> Box<dyn PyLoop> {
        Box::new(self.clone())
    }
}
