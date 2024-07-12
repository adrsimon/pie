use std::env::Args;
use async_trait::async_trait;
use crate::errors::{CommandError, ParseError};
use crate::errors::ParseError::CommandNotFound;
use crate::installer::Installer;

#[async_trait]
pub trait CommandHandler {
    fn parse(&mut self, args: &mut Args) -> Result<(), ParseError>;
    async fn execute(&mut self) -> Result<(), CommandError>;
}

pub async fn handle_args(mut args: Args) -> Result<(), ParseError> {
    args.next();

    let command = match args.next() {
        Some(c) => c,
        None => {
            println!("Please provide a command.");
            return Ok(());
        }
    };

    let mut command_handler: Box<dyn CommandHandler> = match command.to_lowercase().as_str() {
        "install" => Box::new(Installer::default()),
        _ => return Err(CommandNotFound(command.to_string()))
    };

    command_handler.parse(&mut args)?;
    let command_result = command_handler.execute().await;

    if let Err(e) = command_result {
        println!("Command error : {e}")
    }
    Ok(())
}
