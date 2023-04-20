---
title: "Rust: the Actix actor framework for building services"
date: 2023-04-18T15:05:19+01:00
draft: false
tags:
- Rust
- Actix
---

Actix is a Rust crate that provides a framework for developing concurrent applications using the [Actor Model](https://en.wikipedia.org/wiki/Actor_model), a popular design pattern for writing complex concurrent applications. I have found it a useful abstraction due to the way it allows you to decompose your service into discrete _actors_, each responsible for some subset of the service logic, and communicate between these with strongly-typed messages. In particular I think it is useful in Rust as it allows you to avoid some of the noise that comes with managing synchronisation of shared resource manually.

It should be noted that the Actix actor framework is the foundation of Actix Web, a web-services framework build, but I'm only examining the former here.

There are many crates that provide actor model abstractions for rust, and I don't claim to know which is the best or most fully-featured. However, I can say that I have worked with Actix in anger and found it to be relatively intuitive, performant, and flexible.

In this post, I'll give a brief breakdown of the steps to build a simple Actix service and play with different execution modes.

## 1. Getting started

Let's get started with a boilerplate Rust program, to which we'll add the Actix dependency.

```sh
cargo new actix-demo
cd actix-demo
cargo add actix
```

Great! Our `Cargo.toml` now looks like:

```rust
[package]
name = "actix-demo"
version = "0.1.0"
edition = "2021"

# See more keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html

[dependencies]
actix = "0.13.0"
```

## 2. Creating an Actor

Let's create our first actor, a very simple one that will provide some arithmetic operations.

Alongside the `main.rs`, let's create `arithmetic_service.rs`:

```rust
use actix::{Actor, Context};

pub(crate) struct ArithmeticService;

impl Actor for ArithmeticService {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        println!("ArithmeticService is running");
    }
}
```

Implementing the `started` and `stopped` functions is optional, but seems useful in this context.

To get this up and running, we need to decorate our `main` function with `actix::main` macro (which essentially starts the async runtime), declare it as async (more on this later), and then start our service (requiring the `Actor` trait to be in scope):

```rust
mod arithmetic_service;

use crate::arithmetic_service::{GreetingService};
use actix::{Actor};

#[actix::main]
async fn main() {
    ArithmeticService.start();
}
```

And voila:

```sh
cargo run
Finished dev [unoptimized + debuginfo] target(s) in 0.79s
     Running `target/debug/actix-demo`
ArithmeticService is running
ArithmeticService stopped
```

## 3. Defining a message

Actix uses the concept of "messages" for communicating with actors. While more complex to setup and use than, say, a function call, messages are a big part of how the framework provides a separation of concerns, safe concurrency and asynchronicity.

To get us started, let's give our service functionality for returning the square of an input (**yawn**). So we define the messages as a struct with the `Message` derivation, and `rtype` macro to define the return type in `arithmetic_service.rs`. That the result type must be declared by name (as a string) is not something I'm wildly keen on, but rest-assured the compiler complains if this is not a valid type.

```rust
#[derive(Message)]
#[rtype(result = "i64")]
pub(crate) struct Square{
    pub input: i64,
}
```

Now we make the service able to "handle" the message, by implementing the `Handler` trait:

```rust
impl Handler<Square> for ArithmeticService {
    type Result = i64;

    fn handle(&mut self, msg: Square, _ctx: &mut Self::Context) -> Self::Result {
        msg.input * msg.input
    }
}
```

## 4. Sending a message to the service

We can now send a `Square` message to the service from `main`. To do so, when we create the service actor, we need to keep a "address" for the service, which allows us to speak to the service.

We send the message using the `send` function, which returns a promise so must be `await`ed; and we handle the `Result` of the send (which would be a error in various scenarios, e.g. the service is not running, or its message queue is full; topics for another day).

Finally, to make things a little cleaner, I'll add the [`derive-new`](https://github.com/nrc/derive-new) crate and derive this for the `Square` struct to allow creating the message concisely.

```rust
#[actix::main]
async fn main() {
    let service_address = ArithmeticService.start();
    let result = service_address.send(Square::new(5_i64)).await;

    match result {
        Ok(square) => println!("Square was: {}", square),
        _ => println!("Communication with the actor has failed"),
    }
}
```

And we get:

```sh
cargo run
Finished dev [unoptimized + debuginfo] target(s) in 0.79s
     Running `target/debug/actix-demo`
ArithmeticService is running
Square was: 25
```

Nice! A couple of points about this:

- Using the `send` function is only one option, there are also other options `do_send` and `try_send` which have different semantics around synchronicity and error handling.
- I was a little surprised to not see the `ArithmeticService stopped` message in the stderr; I see the stopped function invoked in other applications I've written. For this post I'm not too interested in investigating whether this is a bug in Actix, or whether there is something I'm missing in the disposal of the resources. The documentation indicates that the service should be torn down when there are no addresses left active, but dropping the address used in `main` does not seem to help.

## 5. Concurrency

To play with the concurrency of Actix, let's first modify our service to simulate a more interesting application than just calculating squares. If we add a delay and some logging into the `Square` message handler:

```rust
    fn handle(&mut self, msg: Square, _ctx: &mut Self::Context) -> Self::Result {
        println!("started processing Square({})", msg.input);
   
        // Simulate some work being done by waiting for n seconds, where n is the input value
        let delay_in_seconds = msg.input.try_into().unwrap();
        sleep(Duration::from_secs(delay_in_seconds));

        println!("finished processing Square({})", msg.input);

        msg.input * msg.input
    }
```

And now let's send a few messages to the service in `main`. Let's do this by collecting the futures returned from calls to `send` and awaiting them out-of-order. The `FuturesUnordered` struct is a useful tool for this purpose, essentially a mechanism for polling futures and responding as they complete:

```rust
    let mut futures = FuturesUnordered::new();
    futures.push(service_address.send(Square::new(5_i64)));
    futures.push(service_address.send(Square::new(2_i64)));
    futures.push(service_address.send(Square::new(3_i64)));

    while let Some(send_result) = futures.next().await {
        match send_result {
            Ok(square) => println!("Got square: {}", square),
            Err(err) => println!("Error communicating with service: {}", err),
        };
    }
```

What happens when we run this?

```sh
ArithmeticService is running
started processing Square(5)
finished processing Square(5)
started processing Square(2)
finished processing Square(2)
started processing Square(3)
finished processing Square(3)
Got square: 25
Got square: 4
Got square: 9
```

What's happening here? Actix uses a single-threaded asynchronous event-loop for all it's processing. This means that both the message handling code and the main function waiting on those futures are all running on the same single thread. In particular, the blocking calls to `sleep` in the message handler are a problem for this processing model.

While we're talking about threads, let's capture some thread IDs in our logging to introspect a little.

The alternative is to run our message handling asynchronously, so as not to block the event loop. If we change our `handle` function to use an asynchronous `sleep` from `tokio`, we can send this to the event loop and wait for it to complete without blocking:

```rust
    fn handle(&mut self, msg: Square, ctx: &mut Self::Context) -> Self::Result {
        println!(
            "started processing Square({}), on {:?}",
            msg.input,
            thread::current().id(),
        );

        let processing_task = async move {
            let delay_in_seconds = msg.input.try_into().unwrap();
            sleep(Duration::from_secs(delay_in_seconds)).await;
        };

        ctx.wait(processing_task.into_actor(self));

        println!("finished processing Square({})", msg.input);
        msg.input * msg.input
    }
```

The magic incantation here is `ctx.wait(processing_task.into_actor(self));`, that executes `processing_task` on whatever execution environment is available to the actor context. This is nice as in this context, we don't need to know about the internals of this, we just tell Actix to run it. In fact, the execution environment in this case comes from what Actix terms an _Arbiter_. In this case, we are using the standard _System Arbiter_, as we have not specified anything else.

How does our program behave now?

```sh
Running main on thread: ThreadId(1)
ArithmeticService is running
started processing Square(5) on ThreadId(1)
finished processing Square(5)
Got square: 25
started processing Square(2) on ThreadId(1)
finished processing Square(2)
Got square: 4
started processing Square(3) on ThreadId(1)
finished processing Square(3)
Got square: 9
ArithmeticService stopped
```

Great! We get our message responses as soon as they are ready, and they are still guaranteed to be in-order, which may be a requirement of whatever system we are building. Notice everything is running on the same thread.

## 6. Synchronous processing with multiple threads

As a final modification to this example, let's switch from using this single-threaded asynchronous execution environment to a multi-threaded synchronous one. This will change the behaviour such that messages are no longer responded to in FIFO order, but as soon as they complete.

To do this we use the Actix `SyncArbiter` in place of the implicit system arbiter. When we create our actor using this, we can specify the number of threads to create for processing, each of which will have it's own Actor to process messages.

In `main`, we swap out the way we create our service:

```rust
    let processing_threads = 3;
    let service_address = SyncArbiter::start(processing_threads, || ArithmeticService);
```

And we change the type of `Context` used by our service:

```rust
    type Context = SyncContext<Self>;
```

`SyncContext`, being a context designed for synchronous processing, does not allow us to `spawn` or `wait` for futures, so we replace our `tokio` sleep with a synchronous equivalent:

```rust
    fn handle(&mut self, msg: Square, _ctx: &mut Self::Context) -> Self::Result {
        println!(
            "started processing Square({}), on {:?}",
            msg.input,
            thread::current().id(),
        );

        let delay_in_seconds = msg.input.try_into().unwrap();
        thread::sleep(Duration::from_secs(delay_in_seconds));

        println!("finished processing Square({})", msg.input);
        msg.input * msg.input
    }
```

And we're done. Let's see how this behaves:

```sh
Running main on thread: ThreadId(1)
ArithmeticService is running
ArithmeticService is running
ArithmeticService is running
started processing Square(5), on ThreadId(4)
started processing Square(2), on ThreadId(3)
started processing Square(3), on ThreadId(2)
finished processing Square(2)
Got square: 4
finished processing Square(3)
Got square: 9
finished processing Square(5)
Got square: 25
ArithmeticService stopped
ArithmeticService stopped
ArithmeticService stopped
```

Presto. We see that three `ArithmeticService`s are started on separate threads, and each one processes one of the messages we send. All three messages are processed concurrently, and their responses are returned when ready, completing out-of-order.

## Conclusions

For those that have made it this far, hopefully this post has been a useful exploration of Actix and how it works, and the different ways in which it can operate.
