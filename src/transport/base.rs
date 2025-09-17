use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::processors::frame::FrameProcessor;
use crate::task_manager::TaskManager;

/// Base trait for transport implementations.
///
/// Provides the foundation for transport classes that handle media streaming,
/// including input and output frame processors for audio and video data.
#[async_trait]
pub trait BaseTransport: Send + Sync {
    /// Get the input frame processor for this transport.
    ///
    /// Returns the frame processor that handles incoming frames.
    fn input(&self) -> Arc<Mutex<FrameProcessor>>;

    /// Get the output frame processor for this transport.
    ///
    /// Returns the frame processor that handles outgoing frames.
    fn output(&self) -> Arc<Mutex<FrameProcessor>>;

    /// Get the name of this transport instance.
    fn name(&self) -> Option<&str>;

    /// Get the input processor name.
    fn input_name(&self) -> Option<&str>;

    /// Get the output processor name.
    fn output_name(&self) -> Option<&str>;
}

/// Base implementation struct for transport implementations.
///
/// Provides a concrete implementation of common transport functionality
/// that can be embedded in specific transport implementations.
pub struct BaseTransportImpl {
    /// Optional name for the transport instance
    name: Option<String>,

    /// Optional name for the input processor
    input_name: Option<String>,

    /// Optional name for the output processor
    output_name: Option<String>,

    /// Input frame processor
    input_processor: Arc<Mutex<FrameProcessor>>,

    /// Output frame processor
    output_processor: Arc<Mutex<FrameProcessor>>,
}

impl BaseTransportImpl {
    /// Initialize a new base transport implementation.
    ///
    /// # Arguments
    ///
    /// * `name` - Optional name for the transport instance
    /// * `input_name` - Optional name for the input processor
    /// * `output_name` - Optional name for the output processor
    /// * `task_manager` - Task manager for the frame processors
    pub fn new(
        name: Option<String>,
        input_name: Option<String>,
        output_name: Option<String>,
        task_manager: Arc<TaskManager>,
    ) -> Self {
        let input_processor_name = input_name.clone().unwrap_or_else(|| "input".to_string());
        let output_processor_name = output_name.clone().unwrap_or_else(|| "output".to_string());

        let input_processor = Arc::new(Mutex::new(FrameProcessor::new(
            input_processor_name,
            Arc::clone(&task_manager),
        )));

        let output_processor = Arc::new(Mutex::new(FrameProcessor::new(
            output_processor_name,
            Arc::clone(&task_manager),
        )));

        Self {
            name,
            input_name,
            output_name,
            input_processor,
            output_processor,
        }
    }

    /// Create a builder for constructing a BaseTransportImpl
    pub fn builder() -> BaseTransportBuilder {
        BaseTransportBuilder::new()
    }
}

#[async_trait]
impl BaseTransport for BaseTransportImpl {
    fn input(&self) -> Arc<Mutex<FrameProcessor>> {
        Arc::clone(&self.input_processor)
    }

    fn output(&self) -> Arc<Mutex<FrameProcessor>> {
        Arc::clone(&self.output_processor)
    }

    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    fn input_name(&self) -> Option<&str> {
        self.input_name.as_deref()
    }

    fn output_name(&self) -> Option<&str> {
        self.output_name.as_deref()
    }
}

/// Builder for constructing BaseTransportImpl instances
pub struct BaseTransportBuilder {
    name: Option<String>,
    input_name: Option<String>,
    output_name: Option<String>,
    task_manager: Option<Arc<TaskManager>>,
}

impl BaseTransportBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            name: None,
            input_name: None,
            output_name: None,
            task_manager: None,
        }
    }

    /// Set the transport name
    pub fn with_name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the input processor name
    pub fn with_input_name<S: Into<String>>(mut self, input_name: S) -> Self {
        self.input_name = Some(input_name.into());
        self
    }

    /// Set the output processor name
    pub fn with_output_name<S: Into<String>>(mut self, output_name: S) -> Self {
        self.output_name = Some(output_name.into());
        self
    }

    /// Set the task manager
    pub fn with_task_manager(mut self, task_manager: Arc<TaskManager>) -> Self {
        self.task_manager = Some(task_manager);
        self
    }

    /// Build the BaseTransportImpl instance
    pub fn build(self) -> Result<BaseTransportImpl, &'static str> {
        let task_manager = self.task_manager.ok_or("TaskManager is required")?;
        Ok(BaseTransportImpl::new(
            self.name,
            self.input_name,
            self.output_name,
            task_manager,
        ))
    }
}

impl Default for BaseTransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_manager::TaskManagerConfig;

    #[tokio::test]
    async fn test_base_transport_creation() {
        let task_manager = Arc::new(TaskManager::new(TaskManagerConfig::default()));
        let transport = BaseTransportImpl::new(
            Some("test_transport".to_string()),
            Some("test_input".to_string()),
            Some("test_output".to_string()),
            task_manager,
        );

        assert_eq!(transport.name(), Some("test_transport"));
        assert_eq!(transport.input_name(), Some("test_input"));
        assert_eq!(transport.output_name(), Some("test_output"));
    }

    #[tokio::test]
    async fn test_base_transport_builder() {
        let task_manager = Arc::new(TaskManager::new(TaskManagerConfig::default()));
        let transport = BaseTransportImpl::builder()
            .with_name("builder_transport")
            .with_input_name("builder_input")
            .with_output_name("builder_output")
            .with_task_manager(task_manager)
            .build()
            .expect("Failed to build transport");

        assert_eq!(transport.name(), Some("builder_transport"));
        assert_eq!(transport.input_name(), Some("builder_input"));
        assert_eq!(transport.output_name(), Some("builder_output"));
    }

    #[tokio::test]
    async fn test_base_transport_processors() {
        let task_manager = Arc::new(TaskManager::new(TaskManagerConfig::default()));
        let transport = BaseTransportImpl::new(None, None, None, task_manager);

        let input = transport.input();
        let output = transport.output();

        // Verify we can access the processors
        let input_guard = input.lock().await;
        let output_guard = output.lock().await;

        assert_eq!(input_guard.name(), "input");
        assert_eq!(output_guard.name(), "output");
    }

    #[tokio::test]
    async fn test_base_transport_default_names() {
        let task_manager = Arc::new(TaskManager::new(TaskManagerConfig::default()));
        let transport = BaseTransportImpl::new(None, None, None, task_manager);

        assert_eq!(transport.name(), None);
        assert_eq!(transport.input_name(), None);
        assert_eq!(transport.output_name(), None);

        // But the processors should have default names
        let input = transport.input();
        let output = transport.output();

        let input_guard = input.lock().await;
        let output_guard = output.lock().await;

        assert_eq!(input_guard.name(), "input");
        assert_eq!(output_guard.name(), "output");
    }

    #[tokio::test]
    async fn test_builder_without_task_manager() {
        let result = BaseTransportImpl::builder().with_name("test").build();

        assert!(result.is_err());
        if let Err(error) = result {
            assert_eq!(error, "TaskManager is required");
        }
    }
}
