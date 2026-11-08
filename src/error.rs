use std::fmt;

#[derive(Debug)]
pub enum Error {
    ProductNotFound(u32),
    CustomerNotFound(u32),
    Invalid(String),
    Io(std::io::Error),
    Json(serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ProductNotFound(id) => write!(f, "produto {id} não encontrado"),
            Error::CustomerNotFound(id) => write!(f, "cliente {id} não encontrado"),
            Error::Invalid(message) => f.write_str(message),
            Error::Io(error) => write!(f, "falha ao ler ou gravar o arquivo de dados: {error}"),
            Error::Json(error) => write!(f, "arquivo de dados inválido: {error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error::Json(error)
    }
}
