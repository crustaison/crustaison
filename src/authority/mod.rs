// Authority Module - Immutable Safety Layer
//
// This module contains the safety-critical components that the agent
// CANNOT modify. These enforce authentication, policies, and execution
// boundaries.

pub mod gateway;
pub mod executor;
pub mod policy;

pub use gateway::{Gateway, GatewayMessage, NormalizedMessage};
pub use executor::{Executor, Command, ExecutionResult};
pub use policy::Policy;
