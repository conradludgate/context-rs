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

struct Deadline(Instant);
struct Expired;

impl Deadline {
    async fn expired() -> Result<(), Expired> {
        // try get the deadline if it was set
        let Some(Deadline(deadline)) = get_value() else { return Ok(()) };
        if deadline > Instant::now() { Ok(()) } else { Err(expired) } 
    }
}

async some_function() -> Result<(), Expired> {
    Deadline::expired()?

    // do some fancy work

    Deadline::expired()?

    // do some more work

    Ok(())
}

async some_other_function() -> Result<(), Expired> {
    loop {
        some_function().await
    }
}

// timeout in 5 seconds
let deadline = Instant::now() + Duration::from_secs(5);

tokio::spawn(some_other_function.provide_ref(&Deadline(deadline)));
```

If you want to pass something more interesting down the stack but need ownership,
you can use the `provide_value` + `take_value` pair of methods. This avoids the `Clone`
bound that `get_value` requires.

Lastly, if you only need to access the value temporarily, you can use the `provide_ref`+`with_ref`
flow. This will accept a closure with the ref provided for a short lived lifetime.
