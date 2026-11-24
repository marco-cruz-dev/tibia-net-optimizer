use std::fmt;
use std::io;

/// Errors produced by the optimizer core and its OS adapters.
#[derive(Debug)]
pub enum Error {
    /// The operation needs Administrator rights (e.g. writing HKLM).
    AccessDenied,
    /// An external command ran but reported failure.
    CommandFailed { program: String, message: String },
    /// Output from an external tool could not be understood.
    Parse(String),
    /// Any other I/O failure.
    Io(io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AccessDenied => write!(f, "access denied: run as Administrator"),
            Error::CommandFailed { program, message } => {
                write!(f, "`{program}` failed: {message}")
            }
            Error::Parse(msg) => write!(f, "could not parse output: {msg}"),
            Error::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::PermissionDenied {
            Error::AccessDenied
        } else {
            Error::Io(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_denied_io_error_maps_to_access_denied() {
        let e: Error = io::Error::from(io::ErrorKind::PermissionDenied).into();
        assert!(matches!(e, Error::AccessDenied));
    }

    #[test]
    fn other_io_errors_are_preserved() {
        let e: Error = io::Error::from(io::ErrorKind::NotFound).into();
        assert!(matches!(e, Error::Io(ref inner) if inner.kind() == io::ErrorKind::NotFound));
    }
}
