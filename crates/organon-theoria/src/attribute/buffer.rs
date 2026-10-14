//! Storage supplied to a const parser: counted first, then allocated by its caller.
use super::ParseError;

/// A parser's variable-length storage. Static attributes own an exact-sized array;
/// dynamic attributes borrow a slice allocated by the generated entry point.
/// The counting pass runs the same parser but never executes its returned builder.
pub struct ParseBuffer<'a, T: Copy, const N: usize> {
    storage: Storage<'a, T, N>,
    len: usize,
}

enum Storage<'a, T: Copy, const N: usize> {
    Count(&'a mut usize),
    Array([Option<T>; N]),
    Slice(&'a mut [Option<T>]),
}

impl<'a, T: Copy, const N: usize> ParseBuffer<'a, T, N> {
    #[doc(hidden)]
    pub const fn counting(count: &'a mut usize) -> Self {
        Self {
            storage: Storage::Count(count),
            len: 0,
        }
    }

    #[doc(hidden)]
    pub const fn array() -> Self {
        Self {
            storage: Storage::Array([None; N]),
            len: 0,
        }
    }

    #[doc(hidden)]
    pub fn slice(storage: &'a mut [Option<T>]) -> Self {
        Self {
            storage: Storage::Slice(storage),
            len: 0,
        }
    }

    pub const fn push(&mut self, value: T) -> Result<(), ParseError> {
        let storage = match &mut self.storage {
            Storage::Count(count) => {
                **count += 1;
                self.len += 1;
                return Ok(());
            }
            Storage::Array(storage) => storage.as_mut_slice(),
            Storage::Slice(storage) => storage,
        };
        if self.len == storage.len() {
            return Err(ParseError::new("parser storage is too small"));
        }
        storage[self.len] = Some(value);
        self.len += 1;
        Ok(())
    }

    /// Read parsed entries when constructing the Source. This performs no parsing.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        let storage = match &self.storage {
            Storage::Count(_) => panic!("a counting parser cannot construct a value"),
            Storage::Array(storage) => storage.as_slice(),
            Storage::Slice(storage) => storage,
        };
        storage[..self.len]
            .iter()
            .map(|entry| entry.as_ref().expect("parser entry initialized"))
    }
}
