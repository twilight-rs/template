# Twilight Template

An opinionated [Twilight] bot template.

The template is built in a modular fashion and provides the following features:

* Gateway session resumption between processes
* Clean shutdown that awaits event handler completion
* An administrative `/restart <shard>` command

## Usage

1. Install [cargo-generate]: `cargo install cargo-generate`
1. Create a bot based upon this template: `cargo generate twilight-rs/template`

[cargo-generate]: https://github.com/cargo-generate/cargo-generate
[Twilight]: https://github.com/twilight-rs/twilight
