use std::error::Error as StdError;
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)] // Allow the use of "{:?}" format specifier
pub struct Error {
    description: String,
    cause: Option<Box<dyn StdError + 'static>>,
}

// Allow the use of "{}" format specifier
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.cause {
            Some(cause) => write!(f, "{} caused by: {}", &self.description, cause),
            _ => write!(f, "{}", &self.description),
        }
    }
}

// Allow this type to be treated like an error
impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.cause.as_ref().map(|e| e.as_ref())
    }
}

// Support converting system errors into our custom error.
// This trait is used in `try!`.
impl Error {
    pub fn new<E: StdError + 'static>(description: &str, cause: E) -> Self {
        Self {
            description: description.to_string(),
            cause: Some(Box::new(cause)),
        }
    }
    pub fn from_str(description: &str) -> Self {
        Self {
            description: description.to_string(),
            cause: None,
        }
    }
}
