use std::fmt;
use std::io;

#[derive(Debug)]
#[allow(dead_code)]
pub enum Error {
    Usage(String),
    Io {
        path: Option<String>,
        source: io::Error,
    },
    Query {
        message: String,
    },
    Config(String),
    BrokenPipe,
    #[allow(dead_code)]
    Interrupted,
}

impl Error {
    pub fn usage(message: impl Into<String>) -> Self {
        Error::Usage(message.into())
    }

    #[allow(dead_code)] // reserved for config-file loading
    pub fn config(message: impl Into<String>) -> Self {
        Error::Config(message.into())
    }

    pub fn query(message: impl Into<String>) -> Self {
        Error::Query {
            message: message.into(),
        }
    }

    pub fn io(path: Option<&str>, source: io::Error) -> Self {
        Error::Io {
            path: path.map(str::to_string),
            source,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Usage(_) | Error::Query { .. } | Error::Config(_) => 1,
            Error::Io { .. } => 2,
            Error::BrokenPipe => 3,
            Error::Interrupted => 130,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(message) | Error::Config(message) => f.write_str(message),
            Error::Query { message } => f.write_str(message),
            Error::Io {
                path: Some(path),
                source,
            } => write!(f, "cannot open '{path}': {source}"),
            Error::Io { path: None, source } => write!(f, "{source}"),
            Error::BrokenPipe => f.write_str("broken pipe"),
            Error::Interrupted => f.write_str("interrupted"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(source: io::Error) -> Self {
        if source.kind() == io::ErrorKind::BrokenPipe {
            Error::BrokenPipe
        } else {
            Error::Io { path: None, source }
        }
    }
}
