// Copyright (C) 2016 Pietro Albini
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::io;
use std::fmt;
use std::net;
use std::error::Error;

use rustc_serialize::json;


pub type FisherResult<T> = Result<T, FisherError>;


#[derive(Debug)]
pub enum ErrorKind {
    ProviderNotFound(String),
    InvalidInput(String),
    HookExecutionFailed(Option<i32>, Option<i32>),
    WebApiStartFailed(String),

    // Derived errors
    IoError(io::Error),
    JsonError(json::DecoderError),
    AddrParseError(net::AddrParseError),
}


#[derive(Debug)]
pub struct FisherError {
    kind: ErrorKind,

    // Additional information
    file: Option<String>,
    line: Option<u32>,
    hook: Option<String>,
}

impl FisherError {

    pub fn new(kind: ErrorKind) -> Self {
        FisherError {
            kind: kind,

            // Those can be filled after
            file: None,
            line: None,
            hook: None,
        }
    }

    pub fn set_file(&mut self, file: String) {
        self.file = Some(file);
    }

    pub fn set_line(&mut self, line: u32) {
        self.line = Some(line);
    }

    pub fn location(&self) -> Option<String> {
        if let Some(file) = self.file.clone() {
            if let Some(line) = self.line {
                Some(format!("file {}, line {}", file, line))
            } else {
                Some(format!("file {}", file))
            }
        } else {
            None
        }
    }

    pub fn set_hook(&mut self, hook: String) {
        self.hook = Some(hook);
    }

    pub fn processing(&self) -> Option<String> {
        self.hook.clone()
    }

    #[cfg(test)]
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }
}


impl Error for FisherError {

    fn description(&self) -> &str {
        match self.kind {
            ErrorKind::ProviderNotFound(..) =>
                "provider not found",
            ErrorKind::HookExecutionFailed(..) =>
                "hook returned non-zero exit code",
            ErrorKind::InvalidInput(..) =>
                "invalid input",
            ErrorKind::WebApiStartFailed(..) =>
                "failed to start the Web API",
            ErrorKind::IoError(ref error) =>
                error.description(),
            ErrorKind::JsonError(ref error) =>
                error.description(),
            ErrorKind::AddrParseError(ref error) =>
                error.description(),
        }
    }

    fn cause(&self) -> Option<&Error> {
        match self.kind {
            ErrorKind::IoError(ref error) => Some(error as &Error),
            ErrorKind::JsonError(ref error) => Some(error as &Error),
            _ => None,
        }
    }
}

impl fmt::Display for FisherError {

    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Get the correct description for the error
        let description = match self.kind {

            ErrorKind::ProviderNotFound(ref provider) =>
                format!("Provider {} not found", provider),

            ErrorKind::HookExecutionFailed(exit_code_opt, signal_opt) =>
                if let Some(exit_code) = exit_code_opt {
                    // The hook returned an exit code
                    format!("hook returned non-zero exit code: {}", exit_code)
                } else if let Some(signal) = signal_opt {
                    // The hook was killed
                    format!("hook stopped with signal {}", signal)
                } else {
                    // This shouldn't happen...
                    "hook execution failed".to_string()
                },

            ErrorKind::InvalidInput(ref error) =>
                format!("invalid input: {}", error),

            ErrorKind::WebApiStartFailed(ref error) =>
                format!("{}", error),

            ErrorKind::IoError(ref error) =>
                format!("{}", error),

            // The default errors of rustc_serialize are really ugly btw
            ErrorKind::JsonError(ref error) => {
                use rustc_serialize::json::DecoderError;
                use rustc_serialize::json::ParserError;

                let message = match *error {

                    DecoderError::MissingFieldError(ref field) =>
                        format!("missing required field: {}", field),

                    DecoderError::ExpectedError(ref expected, ref found) =>
                        format!("expected {}, found {}", expected, found),

                    DecoderError::ParseError(ref pe) => match *pe {

                        ParserError::IoError(ref io_error) =>
                            format!("{}", io_error),

                        ParserError::SyntaxError(ref code, ref r, ref c) => {
                            let msg = json::error_str(code.clone());
                            format!("{} (line {}, column {})", msg, r, c)
                        },

                    },

                    _ => format!("{}", error),
                };

                format!("JSON error: {}", message)
            },

            ErrorKind::AddrParseError(ref error) =>
                format!("{}", error),
        };

        write!(f, "{}", description)
    }
}


macro_rules! derive_error {
    ($from:path, $to:path) => {
        impl From<$from> for FisherError {

            fn from(error: $from) -> Self {
                FisherError::new($to(error))
            }
        }
    };
}


impl From<ErrorKind> for FisherError {

    fn from(error: ErrorKind) -> Self {
        FisherError::new(error)
    }
}


derive_error!(io::Error, ErrorKind::IoError);
derive_error!(json::DecoderError, ErrorKind::JsonError);
derive_error!(net::AddrParseError, ErrorKind::AddrParseError);


pub fn print_err<T>(result: Result<T, FisherError>) -> Result<T, FisherError> {
    // Show a nice error message
    if let Err(ref error) = result {
        println!("{} {}",
            ::ansi_term::Colour::Red.bold().paint("Error:"),
            error,
        );
        if let Some(location) = error.location() {
            println!("{} {}",
                ::ansi_term::Colour::Yellow.bold().paint("Location:"),
                location,
            );
        }
        if let Some(hook) = error.processing() {
            println!("{} {}",
                ::ansi_term::Colour::Yellow.bold().paint("While processing:"),
                hook,
            );
        }
    }

    result
}


pub fn unwrap<T>(result: Result<T, FisherError>) -> T {
    // Print the error message if necessary
    match print_err(result) {
        Err(..) => {
            ::std::process::exit(1);
        },
        Ok(t) => {
            return t;
        }
    }
}
