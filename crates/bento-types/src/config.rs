/// Trait for application-level configuration.
/// This trait should be implemented by application-specific config structs
/// that need to be passed to processors and services.
pub trait AppConfigTrait: Send + Sync + std::fmt::Debug {
    /// Enable downcasting to concrete config type
    fn as_any(&self) -> &dyn std::any::Any;
}
