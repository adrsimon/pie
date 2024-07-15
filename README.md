# pie

A fast and reliable Node.js package manager written in Rust.

## Installation
First of all, you **need** to have Rust installed on your machine. You can install it by following the instructions on the [official website](https://www.rust-lang.org/tools/install).

Then you can install the package by following these steps:
- Clone the repository
- Build the package with `cargo build --release`
- _Eventually_ add the target/release folder to your PATH.
- Run the package with `.target/release/pie <command> <package>@<version>`

## What can it do?

Not much for the moment. The project is at its very early stages.

It can install packages from the npm registry. Here is a list of commands:
- `install` - installs a package from the npm registry. Example: `pie install express@4.17.1`

## What's next?

Here is a sort of **roadmap** of what I want to implement in the future:

- Support of a `package.json`, and a `package-lock.json` file in the project directory
- An `uninstall` and an `update` command
- 
- Help messages for each command
- _More to come..._

## Known issues

- Problems parsing package names when they contain a `@` symbol, such as `@babel/core`
- Problems retrieving lockfile when package contains a / in its name, such as `@vue/compiler-core`

## Inspiration

The idea to code this project came by watching [conaticus's](https://www.youtube.com/@conaticus) video about creating a package manager in Rust. 
The early stages of the project are _heavily inspired_ by [his work](https://github.com/conaticus/click). A goal is to _detach_ from his work and make this project more personal.