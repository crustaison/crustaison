// Cognition Module - Self-Improving Layer
//
// This module contains the components that CAN evolve and improve:
// - Planner: Generates and evaluates plans
// - MemoryEngine: Manages structured memory
// - Reflection: Self-assessment and improvement

pub mod planner;
pub mod doctrine_loader;
pub mod memory_engine;
pub mod reflection;

pub use planner::{Planner, Plan, PlanningResult};
pub use doctrine_loader::{DoctrineLoader, Doctrine};
pub use memory_engine::{MemoryEngine, MemoryRecord};
pub use reflection::{Reflection, ReflectionEngine};
