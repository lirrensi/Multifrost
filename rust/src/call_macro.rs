//! Ergonomic call API with typed method wrappers and argument helpers
//!
//! This module provides macros and traits to make remote calls ergonomic:
//!
//! ```rust
//! use multifrost::{ParentWorker, call};
//!
//! let result: i64 = worker.call!(add(10, 20)).await?;
//! ```
//!
//! Or use the argument extraction helpers:
//!
//! ```rust
//! use multifrost::ExtractArgs;
//!
//! let args = ExtractArgs::new(args)?;
//! let a: i64 = args.get(0)?;
//! let b: i64 = args.get(1)?;
//! ```

use crate::error::{MultifrostError, Result};
use serde::{Serialize};
use serde_json::Value;
use std::future::Future;

/// Trait for extracting typed arguments from JSON values
pub trait ExtractArgs {
    fn new(args: Vec<Value>) -> Self where Self: Sized;
    fn get<T: DeserializeOwned>(&self, index: usize) -> Result<T>;
    fn take<T: DeserializeOwned>(self, index: usize) -> Result<T>;
}

/// Helper struct for extracting arguments with type safety
#[derive(Debug, Clone)]
pub struct ArgsExtractor {
    args: Vec<Value>,
}

impl ArgsExtractor {
    /// Create a new extractor from raw arguments
    pub fn new(args: Vec<Value>) -> Self {
        Self { args }
    }

    /// Get argument at index with type conversion
    pub fn get<T: DeserializeOwned>(&self, index: usize) -> Result<T> {
        self.args.get(index)
            .ok_or_else(|| MultifrostError::InvalidMessage(format!("Missing argument at index {}", index)))
            .and_then(|v| {
                serde_json::from_value(v.clone())
                    .map_err(MultifrostError::JsonError)
            })
    }

    /// Take (consume) argument at index with type conversion
    pub fn take<T: DeserializeOwned>(mut self, index: usize) -> Result<T> {
        if index >= self.args.len() {
            return Err(MultifrostError::InvalidMessage(format!("Missing argument at index {}", index)));
        }
        let value = self.args.remove(index);
        serde_json::from_value(value)
            .map_err(MultifrostError::JsonError)
    }

    /// Get remaining arguments as a Vec
    pub fn remaining(&self) -> &[Value] {
        &self.args
    }

    /// Get the number of arguments
    pub fn len(&self) -> usize {
        self.args.len()
    }

    /// Check if there are no arguments
    pub fn is_empty(&self) -> bool {
        self.args.is_empty()
    }
}

impl ExtractArgs for ArgsExtractor {
    fn new(args: Vec<Value>) -> Self {
        Self::new(args)
    }

    fn get<T: DeserializeOwned>(&self, index: usize) -> Result<T> {
        self.get::<T>(index)
    }

    fn take<T: DeserializeOwned>(self, index: usize) -> Result<T> {
        self.take::<T>(index)
    }
}

/// Marker trait for types that can be deserialized from JSON
pub trait DeserializeOwned: for<'de> serde::Deserialize<'de> + Sized {}

impl<T: for<'de> serde::Deserialize<'de> + Sized> DeserializeOwned for T {}

/// Trait for types that can be converted to JSON arguments
pub trait ToJsonArg {
    fn to_json(self) -> Value;
}

impl ToJsonArg for Value {
    fn to_json(self) -> Value {
        self
    }
}

impl ToJsonArg for i64 {
    fn to_json(self) -> Value {
        Value::from(self)
    }
}

impl ToJsonArg for i32 {
    fn to_json(self) -> Value {
        Value::from(self)
    }
}

impl ToJsonArg for i16 {
    fn to_json(self) -> Value {
        Value::from(self)
    }
}

impl ToJsonArg for i8 {
    fn to_json(self) -> Value {
        Value::from(self)
    }
}

impl ToJsonArg for u64 {
    fn to_json(self) -> Value {
        Value::from(self)
    }
}

impl ToJsonArg for u32 {
    fn to_json(self) -> Value {
        Value::from(self)
    }
}

impl ToJsonArg for u16 {
    fn to_json(self) -> Value {
        Value::from(self)
    }
}

impl ToJsonArg for u8 {
    fn to_json(self) -> Value {
        Value::from(self)
    }
}

impl ToJsonArg for f64 {
    fn to_json(self) -> Value {
        Value::from(self)
    }
}

impl ToJsonArg for f32 {
    fn to_json(self) -> Value {
        Value::from(self)
    }
}

impl ToJsonArg for bool {
    fn to_json(self) -> Value {
        Value::from(self)
    }
}

impl ToJsonArg for String {
    fn to_json(self) -> Value {
        Value::String(self)
    }
}

impl ToJsonArg for &str {
    fn to_json(self) -> Value {
        Value::String(self.to_string())
    }
}

impl<T: Serialize> ToJsonArg for &T {
    fn to_json(self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

impl<T: Serialize> ToJsonArg for Vec<T> {
    fn to_json(self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

impl<T: Serialize + Clone> ToJsonArg for &[T] {
    fn to_json(self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// Macro to create a typed remote call
///
/// This macro provides ergonomic syntax for making remote calls:
///
/// ```rust
/// # use multifrost::{ParentWorker, call};
/// # use serde::Deserialize;
/// # #[derive(Deserialize)]
/// # struct AddResult { result: i64 }
/// #
/// let result: i64 = worker.call!(add(10, 20)).await?;
/// let result: AddResult = worker.call!(add(10, 20)).await?;
/// ```
///
/// Without the macro (verbose):
/// ```rust
/// # use serde_json::json;
/// let result: i64 = worker.call("add", vec![json!(10), json!(20)]).await?;
/// ```
///
/// # Arguments
///
/// - Function name as identifier
/// - Comma-separated arguments (any type implementing `ToJsonArg`)
/// - Return type annotation (usingturbofish or type inference)
///
/// # Examples
///
/// ```rust
/// // Simple calls
/// let sum: i64 = worker.call!(add(10, 20)).await?;
/// let product: i64 = worker.call!(multiply(7, 8)).await?;
///
/// // With type annotation
/// let result = worker.call!(add(10, 20))<i64>.await?;
///
/// // String arguments
/// let greeting: String = worker.call!(greet("world")).await?;
///
/// // Multiple arguments of different types
/// let result: bool = worker.call!(check(true, 42, "test")).await?;
/// ```
#[macro_export]
macro_rules! call {
    ($worker:expr, $func:ident($($arg:expr),*)) => {
        $worker.call(stringify!($func), vec![$($crate::call_macro::ToJsonArg::to_json($arg)),*])
    };
    ($worker:expr, $func:ident($($arg:expr),*), $($rest:tt)*) => {
        compile_error!("call! macro takes function name and arguments only")
    };
}

/// Macro to create a typed remote call with explicit return type
///
/// This is useful when type inference fails:
///
/// ```rust
/// let result = worker.call_with_type!(add, (10, 20), i64).await?;
/// ```
#[macro_export]
macro_rules! call_with_type {
    ($worker:expr, $func:ident, ($($arg:expr),*), $ret:ty) => {
        $worker.call::<$ret>(stringify!($func), vec![$($crate::call_macro::ToJsonArg::to_json($arg)),*])
    };
}

/// Helper trait for ergonomic async calls
pub trait ErgonomicCall {
    /// Call a function with typed arguments (variadic)
    fn call_fn<R, A>(&self, name: &str, args: A) -> impl Future<Output = Result<R>>
    where
        R: serde::de::DeserializeOwned,
        A: CallArgs;
}

impl<'a> ErgonomicCall for &'a crate::parent::ParentWorker {
    fn call_fn<R, A>(&self, name: &str, args: A) -> impl Future<Output = Result<R>>
    where
        R: serde::de::DeserializeOwned,
        A: CallArgs,
    {
        async move {
            let json_args: Vec<Value> = args.to_json_args();
            self.call(name, json_args).await
        }
    }
}

/// Trait for converting arguments to JSON
pub trait CallArgs {
    fn to_json_args(self) -> Vec<Value>;
}

impl CallArgs for () {
    fn to_json_args(self) -> Vec<Value> {
        vec![]
    }
}

impl<A: ToJsonArg> CallArgs for (A,) {
    fn to_json_args(self) -> Vec<Value> {
        vec![self.0.to_json()]
    }
}

impl<A: ToJsonArg, B: ToJsonArg> CallArgs for (A, B) {
    fn to_json_args(self) -> Vec<Value> {
        vec![self.0.to_json(), self.1.to_json()]
    }
}

impl<A: ToJsonArg, B: ToJsonArg, C: ToJsonArg> CallArgs for (A, B, C) {
    fn to_json_args(self) -> Vec<Value> {
        vec![self.0.to_json(), self.1.to_json(), self.2.to_json()]
    }
}

impl<A: ToJsonArg, B: ToJsonArg, C: ToJsonArg, D: ToJsonArg> CallArgs for (A, B, C, D) {
    fn to_json_args(self) -> Vec<Value> {
        vec![self.0.to_json(), self.1.to_json(), self.2.to_json(), self.3.to_json()]
    }
}

impl<A: ToJsonArg, B: ToJsonArg, C: ToJsonArg, D: ToJsonArg, E: ToJsonArg> CallArgs for (A, B, C, D, E) {
    fn to_json_args(self) -> Vec<Value> {
        vec![self.0.to_json(), self.1.to_json(), self.2.to_json(), self.3.to_json(), self.4.to_json()]
    }
}

impl<A: ToJsonArg, B: ToJsonArg, C: ToJsonArg, D: ToJsonArg, E: ToJsonArg, F: ToJsonArg> CallArgs for (A, B, C, D, E, F) {
    fn to_json_args(self) -> Vec<Value> {
        vec![self.0.to_json(), self.1.to_json(), self.2.to_json(), self.3.to_json(), self.4.to_json(), self.5.to_json()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_args_extractor_get() {
        let args = ArgsExtractor::new(vec![json!(10), json!(20)]);
        let a: i64 = args.get(0).unwrap();
        let b: i64 = args.get(1).unwrap();
        assert_eq!(a, 10);
        assert_eq!(b, 20);
    }

    #[test]
    fn test_args_extractor_take() {
        let args = ArgsExtractor::new(vec![json!(10), json!(20)]);
        let a: i64 = args.take(0).unwrap();
        let b: i64 = args.take(0).unwrap(); // Now at index 0
        assert_eq!(a, 10);
        assert_eq!(b, 20);
    }

    #[test]
    fn test_args_extractor_missing() {
        let args = ArgsExtractor::new(vec![json!(10)]);
        let result = args.get::<i64>(1);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_json_arg_primitives() {
        assert_eq!(5i64.to_json(), json!(5));
        assert_eq!(true.to_json(), json!(true));
        assert_eq!("hello".to_json(), json!("hello"));
        assert_eq!(3.14.to_json(), json!(3.14));
    }

    #[test]
    fn test_call_args_tuples() {
        assert_eq!(().to_json_args(), vec![]);
        assert_eq!((1i64,).to_json_args(), vec![json!(1)]);
        assert_eq!((1i64, 2i64).to_json_args(), vec![json!(1), json!(2)]);
        assert_eq!((1i64, 2i64, 3i64).to_json_args(), vec![json!(1), json!(2), json!(3)]);
    }

    #[test]
    fn test_extract_args_type_conversion() {
        let args = ArgsExtractor::new(vec![json!(42), json!("hello"), json!(true)]);
        let a: i64 = args.get(0).unwrap();
        let b: String = args.get(1).unwrap();
        let c: bool = args.get(2).unwrap();
        assert_eq!(a, 42);
        assert_eq!(b, "hello");
        assert_eq!(c, true);
    }
}