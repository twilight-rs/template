# Twilight Template

A set of opinionated [Twilight] bot templates.

## Usage

1. Install [cargo-generate]: `cargo install cargo-generate`
1. Create a bot based upon a select template: `cargo generate twilight-rs/template`

## Variants

- Single-process: fast restarts
- Multi-process: zero downtime restarts

### Single-process

This template is built in a modular fashion and provides the following features:

* Gateway session resumption between restarts
* Clean shutdown that awaits event handler completion
* An administrative `/restart <shard>` command

### Multi-process

This template is split into `gateway` and `worker` crates, where the `gateway`
forwards events and provides state to the `worker`. It otherwise mirrors the
single-process template:

* gateway:
  * Gateway session resumption between restarts
  * Clean shutdown that awaits event forwarder completion
* worker:
  * Clean shutdown that awaits event handler completion

**Note**: this adds IPC overhead compared to the single-process template. You
may spread the load of the single-process template:

1. Across threads with the multi-threaded tokio runtime
1. Across machines by partitioning your shards (i.e. each machine runs 1/X shards)

[cargo-generate]: https://github.com/cargo-generate/cargo-generate
[Twilight]: https://github.com/twilight-rs/twilight
