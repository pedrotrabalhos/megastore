//! ConectaStore: recomendação de produtos da MegaStore baseada em grafos.

pub mod bench;
pub mod cli;
pub mod error;
pub mod graph;
pub mod recommend;
pub mod seed;
pub mod store;

pub use error::{Error, Result};
