//! Ergonomic call helpers for the v5 router API.
//!
//! ```rust,no_run
//! use multifrost::{call, connect};
//!
//! # #[tokio::main]
//! # async fn main() -> multifrost::Result<()> {
//! let connection = connect("math-service", 5_000).await?;
//! let handle = connection.handle();
//! handle.start().await?;
//! let result: i64 = call!(handle, add(10, 20)).await?;
//! assert_eq!(result, 30);
//! handle.stop().await;
//! # Ok(())
//! # }
//! ```
//!
//! ```rust,no_run
//! use multifrost::ArgsExtractor;
//!
//! fn main() -> multifrost::Result<()> {
//!     let args = ArgsExtractor::new(vec![serde_json::json!(10), serde_json::json!(20)]);
//!     let a: i64 = args.get(0)?;
//!     let b: i64 = args.get(1)?;
//!     assert_eq!(a + b, 30);
//!     Ok(())
//! }
//! ```

use crate::error::{MultifrostError, Result};
use serde::Serialize;
use serde_json::Value;
use std::future::Future;

/// Trait for extracting typed arguments from JSON values
pub trait ExtractArgs {
    fn new(args: Vec<Value>) -> Self
    where
        Self: Sized;
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
        self.args
            .get(index)
            .ok_or_else(|| {
                MultifrostError::InvalidMessage(format!("Missing argument at index {}", index))
            })
            .and_then(|v| serde_json::from_value(v.clone()).map_err(MultifrostError::JsonError))
    }

    /// Take (consume) argument at index with type conversion
    pub fn take<T: DeserializeOwned>(mut self, index: usize) -> Result<T> {
        if index >= self.args.len() {
            return Err(MultifrostError::InvalidMessage(format!(
                "Missing argument at index {}",
                index
            )));
        }
        let value = self.args.remove(index);
        serde_json::from_value(value).map_err(MultifrostError::JsonError)
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

/// Macro to create a typed remote call.
///
/// ```rust,no_run
/// use multifrost::{call, connect};
///
/// # #[tokio::main]
/// # async fn main() -> multifrost::Result<()> {
/// let connection = connect("math-service", 5_000).await?;
/// let handle = connection.handle();
/// handle.start().await?;
/// let sum: i64 = call!(handle, add(10, 20)).await?;
/// assert_eq!(sum, 30);
/// handle.stop().await;
/// # Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! call {
    ($worker:expr, $func:ident($($arg:expr),*)) => {
        $worker.call(stringify!($func), vec![$($crate::ToJsonArg::to_json($arg)),*])
    };
    ($worker:expr, $func:ident($($arg:expr),*), $($rest:tt)*) => {
        compile_error!("call! macro takes function name and arguments only")
    };
}

/// Macro to create a typed remote call with explicit return type.
///
/// ```rust,no_run
/// use multifrost::{call_with_type, connect};
///
/// # #[tokio::main]
/// # async fn main() -> multifrost::Result<()> {
/// let connection = connect("math-service", 5_000).await?;
/// let handle = connection.handle();
/// handle.start().await?;
/// let result: i64 = call_with_type!(handle, add, (10, 20), i64).await?;
/// assert_eq!(result, 30);
/// handle.stop().await;
/// # Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! call_with_type {
    ($worker:expr, $func:ident, ($($arg:expr),*), $ret:ty) => {
        $worker.call::<$ret>(stringify!($func), vec![$($crate::ToJsonArg::to_json($arg)),*])
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

impl<'a> ErgonomicCall for &'a crate::Handle {
    fn call_fn<R, A>(&self, name: &str, args: A) -> impl Future<Output = Result<R>>
    where
        R: serde::de::DeserializeOwned,
        A: CallArgs,
    {
        async move {
            let json_args: Vec<Value> = args.to_json_args();
            self.call::<R>(name, json_args).await
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
        vec![
            self.0.to_json(),
            self.1.to_json(),
            self.2.to_json(),
            self.3.to_json(),
        ]
    }
}

impl<A: ToJsonArg, B: ToJsonArg, C: ToJsonArg, D: ToJsonArg, E: ToJsonArg> CallArgs
    for (A, B, C, D, E)
{
    fn to_json_args(self) -> Vec<Value> {
        vec![
            self.0.to_json(),
            self.1.to_json(),
            self.2.to_json(),
            self.3.to_json(),
            self.4.to_json(),
        ]
    }
}

impl<A: ToJsonArg, B: ToJsonArg, C: ToJsonArg, D: ToJsonArg, E: ToJsonArg, F: ToJsonArg> CallArgs
    for (A, B, C, D, E, F)
{
    fn to_json_args(self) -> Vec<Value> {
        vec![
            self.0.to_json(),
            self.1.to_json(),
            self.2.to_json(),
            self.3.to_json(),
            self.4.to_json(),
            self.5.to_json(),
        ]
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
        // Take consumes the extractor and removes the element at the given index
        // After taking index 0, we get 10 and the remaining is [20]
        let a: i64 = args.take(0).unwrap();
        assert_eq!(a, 10);

        // Test sequential takes with fresh extractors
        let args2 = ArgsExtractor::new(vec![json!(30), json!(40)]);
        let b: i64 = args2.take(1).unwrap(); // Take index 1 (which is 40)
        assert_eq!(b, 40);
    }

    #[test]
    fn test_args_extractor_missing() {
        let args = ArgsExtractor::new(vec![json!(10)]);
        let result = args.get::<i64>(1);
        assert!(result.is_err());
    }

    #[test]
    fn test_args_extractor_remaining_and_len() {
        let args = ArgsExtractor::new(vec![json!(1), json!(2), json!(3)]);
        assert_eq!(args.len(), 3);
        assert!(!args.is_empty());
        assert_eq!(args.remaining(), &[json!(1), json!(2), json!(3)]);
    }

    #[test]
    fn test_args_extractor_take_middle_value() {
        let args = ArgsExtractor::new(vec![json!(10), json!(20), json!(30)]);
        let middle: i64 = args.take(1).unwrap();
        assert_eq!(middle, 20);
    }

    #[test]
    fn test_to_json_arg_primitives() {
        assert_eq!(5i64.to_json(), json!(5));
        assert_eq!(true.to_json(), json!(true));
        assert_eq!("hello".to_json(), json!("hello"));
        assert_eq!(3.14.to_json(), json!(3.14));
    }

    #[test]
    fn test_to_json_arg_collections() {
        let values = vec![1, 2, 3];
        assert_eq!(values.clone().to_json(), json!([1, 2, 3]));
        assert_eq!(values.as_slice().to_json(), json!([1, 2, 3]));
    }

    #[test]
    fn test_call_args_tuples() {
        let empty: () = ();
        let result: Vec<Value> = CallArgs::to_json_args(empty);
        assert_eq!(result, Vec::<Value>::new());
        assert_eq!((1i64,).to_json_args(), vec![json!(1)]);
        assert_eq!((1i64, 2i64).to_json_args(), vec![json!(1), json!(2)]);
        assert_eq!(
            (1i64, 2i64, 3i64).to_json_args(),
            vec![json!(1), json!(2), json!(3)]
        );
        assert_eq!(
            (1i64, 2i64, 3i64, 4i64).to_json_args(),
            vec![json!(1), json!(2), json!(3), json!(4)]
        );
        assert_eq!(
            (1i64, 2i64, 3i64, 4i64, 5i64, 6i64).to_json_args(),
            vec![json!(1), json!(2), json!(3), json!(4), json!(5), json!(6)]
        );
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

    #[test]
    fn test_extract_args_type_conversion_failure() {
        let args = ArgsExtractor::new(vec![json!("not-a-number")]);
        assert!(args.get::<i64>(0).is_err());
    }
}
