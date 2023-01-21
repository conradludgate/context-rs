#![feature(waker_getters, provide_any)]
#![cfg_attr(not(test), no_std)]
#![doc = include_str!("../README.md")]

mod demand;
mod provider;
mod waker;

pub use demand::{get_value, take_value, with_ref};
pub use provider::{ProvideRef, ProvideValue, ProviderFut, ProviderFutExt};
pub use waker::ProviderWaker;

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use super::*;

    #[test]
    fn ctx_with_ref() {
        let val = async {
            // works once
            let upper = with_ref(|s: &str| s.to_uppercase()).await.unwrap();
            // works again
            let len = with_ref(|s: &str| s.len()).await.unwrap();
            format!("{upper}{len}")
        }
        .provide_ref("foo")
        .now_or_never()
        .unwrap();

        assert_eq!(val, "FOO3");
    }

    #[test]
    fn ctx_get_value() {
        let val = async {
            // works once
            let num1 = get_value::<i32>().await.unwrap();
            // works again
            let num2 = get_value::<i32>().await.unwrap();
            num1 + num2
        }
        .provide_ref(&123)
        .now_or_never()
        .unwrap();

        assert_eq!(val, 246);
    }

    #[test]
    fn ctx_take_value() {
        let val = async {
            // works once
            let greeting: &'static str = take_value().await.unwrap();
            // works again
            let subject: &'static str = take_value().await.unwrap();

            // third time has no value
            assert_eq!(take_value::<&'static str>().await, None);

            format!("{greeting}, {subject}!")
        }
        .provide_value("Hello")
        .provide_value("World")
        .now_or_never()
        .unwrap();

        assert_eq!(val, "Hello, World!");
    }
}
