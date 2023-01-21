#![feature(waker_getters, provide_any)]
#![cfg_attr(not(test), no_std)]

mod demand;
mod provider;
mod waker;

pub use demand::{get_value, take_value, with_ref};
pub use provider::ProviderFutExt;
pub use waker::ProviderWaker;

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use super::*;

    #[test]
    fn it_works() {
        let test = async {
            (
                get_value::<usize>().await.unwrap(),
                get_value::<usize>().await.unwrap(),
                take_value::<String>().await.unwrap(),
                take_value::<String>().await.unwrap(),
                take_value::<Vec<u8>>().await.unwrap(),
                take_value::<Vec<u8>>().await,
            )
        };

        let (v0, v1, v2, v3, v4, v5) = test
            .provide_value(vec![1_u8, 2, 3, 4])
            .provide_value("hello world".to_owned())
            .provide_value("goodbye world".to_owned())
            .provide_ref(&123_usize)
            .now_or_never()
            .unwrap();

        // get_value should not remove from context and should be
        // callable again
        assert_eq!(v0, 123);
        assert_eq!(v1, 123);

        // take_value should also work, returning the chain of owned values
        assert_eq!(v2, "hello world");
        assert_eq!(v3, "goodbye world");

        // take_value should return the None if there's no more values in the chain
        assert_eq!(v4, [1, 2, 3, 4]);
        assert_eq!(v5, None);
    }
}
