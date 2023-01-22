# context-rs

Go has a history of suggesting you provide a `ctx context.Context` parameter to
all functions in an async context, such as web servers. This is useful for passing 
deadlines and such down into the callstack, to allow leaf-functions to schedule shutdowns.

Rust already passes a context value automatically for you in all async functions,
this is already named [`Context`](https://doc.rust-lang.org/std/task/struct.Context.html)
but it's heavily under-featured - only providing a 'wake-up' handle.

Making use of the nightly [`Provider API`](https://doc.rust-lang.org/std/any/trait.Provider.html),
we can modify this context to provide values on demand down the callstack. This avoids
using thread_locals, which requires `std`, or passing through a `TypeMap` with every
function call, which is unergonomic and requires `alloc`.

## Examples

A demonstration of an async deadline, using `get_value` and `provide_ref`

```rust
use context_rs::{get_value, ProviderFutExt};
use std::time::{Instant, Duration};

// New type makes it easier to have unique keys in the context
#[derive(Clone)]
struct Deadline(Instant);

#[derive(Debug, PartialEq)]
struct Expired;

impl Deadline {
    // check if the deadline stored in the context has expired
    // returns OK if no deadline is stored.
    async fn expired() -> Result<(), Expired> {
        get_value().await.map(|Deadline(deadline)| {
            // if there is a deadline set, check if it has expired
            if deadline < Instant::now() {
                Err(Expired)
            } else {
                Ok(())
            }
        }).unwrap_or(Ok(())) // or ignore it if no deadline is set
    }
}

// some top level work - agnostic to the context
async fn some_work() -> Result<(), Expired> {
    loop {
        some_nested_function().await?
    }
}

// some deeply nested work, cares about the deadline context
async fn some_nested_function() -> Result<(), Expired> {
    // will acquire the deadline from the context itself
    Deadline::expired().await?;

    // do some logic in here

    Ok(())
}

#[tokio::main]
async fn main() {
    // timeout in 2 seconds
    let deadline = Instant::now() + Duration::from_secs(2);

    let res = some_work().provide_ref(&Deadline(deadline)).await;

    assert_eq!(res, Err(Expired));
}
```

If you want to pass something more interesting down the stack but need ownership,
you can use the `provide_value` + `take_value` pair of methods. This avoids the `Clone`
bound that `get_value` requires.

Lastly, if you only need to access the value temporarily, you can use the `provide_ref`+`with_ref`
flow. This will accept a closure with the ref provided for a short lived lifetime.
