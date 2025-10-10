//! Pipeline implementation for connecting and managing frame processors.
//!
//! This module provides the main Pipeline class that connects frame processors
//! in sequence and manages frame flow between them, along with helper classes
//! for pipeline source and sink operations.

pub mod base;
pub mod pipeline;
pub mod sink;
pub mod source;

pub use base::*;
pub use pipeline::*;
pub use sink::*;
pub use source::*;
