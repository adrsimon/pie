mod command_handler;
mod errors;
mod installer;
mod types;
mod constants;
mod versions;
mod http;

use std::env;

#[tokio::main]
async fn main() {
    let parse_result = command_handler::handle_args(env::args()).await;

    if let Err(err) = parse_result {
        println!("Failed to parse command: {err}");
    }
}
