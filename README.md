# Reference Agent

minionrt is an open-source project aiming to connect software engineering tools with AI agents through a common interface.
This repository contains the source code for a reference agent that speaks the minionrt protocol.

> [!WARNING]
> This project is in an early state of development.
> The agent is currently not very sophisticated.

## Quickstart

- Install the [minionrt CLI](https://github.com/minionrt/cli).
- Clone this repository on your machine:
  ```console
  git clone https://github.com/minionrt/default-minion
  ```
- Navigate to any git repository for testing and run:
  ```console
  minion run --containerfile <path to the default-minion repo>/Containerfile.minion
  ```
  The minionrt CLI will build a container image from the current state of your local clone of the `default-minion` repository.
  This container image will then subsequently be used to run the agent on the git repository in your current working directory.

## License

This project is distributed under the terms of both the MIT license and the Apache License 2.0.
See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT) for details.
