use std::convert;

pub(crate) trait Sealed {}

impl<T, E> Sealed for Result<T, E> {}

#[allow(private_bounds)]
pub trait ResultFlattening: Sealed {
    type Ok;

    type Err;

    /// Converts from `Result<Result<T, E>, E>` to `Result<T, E>`
    /// Taken directly from https://github.com/rust-lang/rust/pull/70140/commits/6fe7867ea6f5f912346d75459499fca88f6ae563
    ///
    /// # Examples
    /// Basic usage:
    /// ```
    /// #![feature(result_flattening)]
    /// let x: Result<Result<&'static str, u32>, u32> = Ok(Ok("hello"));
    /// assert_eq!(Ok("hello"), x.flatten());
    ///
    /// let x: Result<Result<&'static str, u32>, u32> = Ok(Err(6));
    /// assert_eq!(Err(6), x.flatten());
    ///
    /// let x: Result<Result<&'static str, u32>, u32> = Err(6);
    /// assert_eq!(Err(6), x.flatten());
    /// ```
    ///
    /// Flattening once only removes one level of nesting:
    ///
    /// ```
    /// #![feature(result_flattening)]
    /// let x: Result<Result<Result<&'static str, u32>, u32>, u32> = Ok(Ok(Ok("hello")));
    /// assert_eq!(Ok(Ok("hello")), x.flatten());
    /// assert_eq!(Ok("hello"), x.flatten().flatten());
    /// ```
    fn custom_flatten(self) -> Result<Self::Ok, Self::Err>;
}

impl<T, E> ResultFlattening for Result<Result<T, E>, E> {
    type Ok = T;
    type Err = E;

    fn custom_flatten(self) -> Result<T, E> {
        self.and_then(convert::identity)
    }
}
