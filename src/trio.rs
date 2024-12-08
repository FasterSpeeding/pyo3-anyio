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
use once_cell::sync::OnceCell as OnceLock;
use pyo3::exceptions::{PyBaseException, PyRuntimeError};
use pyo3::types::{IntoPyDict, PyDict, PyTuple};
use pyo3::{IntoPy, PyAny, PyErr, PyObject, PyResult, Python, ToPyObject};

use crate::traits::{BoxedFuture, PyLoop};
use crate::ContextWrap;

// TODO: switch to std::sync::OnceLock once https://github.com/rust-lang/rust/issues/74465 is done.
static TRIO_LOW: OnceLock<PyObject> = OnceLock::new();
static WRAP_FUNC: OnceLock<PyObject> = OnceLock::new();


fn import_trio_low(py: Python) -> PyResult<&PyAny> {
    TRIO_LOW
        .get_or_try_init(|| Ok(py.import("trio.lowlevel")?.to_object(py)))
        .map(|value| value.as_ref(py))
}


fn wrap_func(py: Python) -> &PyAny {
    WRAP_FUNC
        .get_or_init(|| {
            let globals = PyDict::new(py);
            py.run(
                r"
async def wrap_func(coro, one_shot, args, kwargs, /):
    try:
        if kwargs:
            result = await coro(*args, **kwargs)

        else:
            result = await coro(*args)

    except BaseException as exc:
        one_shot.set_exception(exc)

    else:
        one_shot.set(result)
            ",
                Some(globals),
                None,
            )
            .unwrap();

            globals.get_item("wrap_func").unwrap().to_object(py)
        })
        .as_ref(py)
}


#[pyo3::pyclass]
struct TrioHook {
    sender: async_oneshot::Sender<PyResult<PyObject>>,
}

#[pyo3::pymethods]
impl TrioHook {
    #[pyo3(signature = (value, /))]
    fn set(&mut self, value: PyObject) {
        self.sender.send(Ok(value)).unwrap();
    }

    #[pyo3(signature = (value, /))]
    fn set_exception(&mut self, value: &PyBaseException) {
        self.sender.send(Err(PyErr::from_value(value))).unwrap();
    }
}

#[pyo3::pyclass]
struct WrapCoro {
    coroutine: PyObject,
}

#[pyo3::pymethods]
impl WrapCoro {
    fn __call__<'p>(&'p self, py: Python<'p>) -> &'p PyAny {
        self.coroutine.as_ref(py)
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
    fn await_py(
        &self,
        context: Option<&PyAny>,
        callback: &PyAny,
        args: &[&PyAny],
        kwargs: Option<&PyDict>,
    ) -> PyResult<BoxedFuture<PyResult<PyObject>>> {
        let py = callback.py();
        let (sender, receiver) = async_oneshot::oneshot::<PyResult<PyObject>>();
        let one_shot = TrioHook { sender }.into_py(py);

        self.call_soon(
            None,
            import_trio_low(py)?.getattr("spawn_system_task")?,
            &[
                wrap_func(py),
                callback,
                one_shot.as_ref(py),
                PyTuple::new(py, args),
                kwargs.map_or_else(|| crate::import_none(py), PyDict::as_ref),
            ],
            Some([("context", context)].into_py_dict(py)),
        )?;

        Ok(Box::pin(async move { receiver.await.unwrap() }))
    }

    fn call_soon(
        &self,
        context: Option<&PyAny>,
        callback: &PyAny,
        args: &[&PyAny],
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        self.token.call_method1(
            callback.py(),
            "run_sync_soon",
            (
                ContextWrap::py(context, callback),
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
        args: &[&PyAny],
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        let py = callback.py();
        let args = &[&[callback], args].concat();
        let wrapped = ContextWrap::py(None, import_trio_low(py)?.getattr("spawn_system_task")?);
        self.call_soon(
            None,
            wrapped.as_ref(py),
            &[
                PyTuple::new(py, args),
                kwargs.map_or_else(|| crate::import_none(py), PyDict::as_ref),
            ],
            Some([("context", context)].into_py_dict(py)),
        )?;

        Ok(())
    }

    fn clone_box(&self) -> Box<dyn PyLoop> {
        Box::new(self.clone())
    }
}
