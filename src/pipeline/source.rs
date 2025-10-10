//! Pipeline source processor that acts as the entry point for a pipeline.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::processors::{
    BaseInterruptionStrategy, FrameCallback, FrameDirection, FrameProcessor, FrameProcessorMetrics,
    FrameProcessorSetup, FrameProcessorTrait,
};
use crate::task_manager::{TaskHandle, TaskManager};
use crate::{BaseClock, FrameType};

/// Type alias for upstream frame handler function
pub type UpstreamFrameHandler = Arc<
    dyn Fn(
            FrameType,
            FrameDirection,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
        + Send
        + Sync,
>;

/// Source processor that forwards frames to an upstream handler.
///
/// This processor acts as the entry point for a pipeline, forwarding
/// downstream frames to the next processor and upstream frames to a
/// provided upstream handler function.
#[derive(Clone)]
pub struct PipelineSource {
    processor: Arc<Mutex<FrameProcessor>>,
    upstream_handler: UpstreamFrameHandler,
}

impl PipelineSource {
    /// Create a new pipeline source with the given upstream handler.
    ///
    /// # Arguments
    /// * `upstream_handler` - Function to handle upstream frames
    /// * `name` - Optional name for the source processor
    /// * `task_manager` - Task manager for handling async operations
    pub fn new(
        upstream_handler: UpstreamFrameHandler,
        name: Option<String>,
        task_manager: Arc<TaskManager>,
    ) -> Self {
        let processor_name = name.unwrap_or_else(|| "PipelineSource".to_string());
        let processor = FrameProcessor::new(processor_name, task_manager);

        Self {
            processor: Arc::new(Mutex::new(processor)),
            upstream_handler,
        }
    }

    /// Get the underlying frame processor
    pub fn processor(&self) -> Arc<Mutex<dyn FrameProcessorTrait>> {
        self.processor.clone()
    }

    // Task management - these need to be async delegated
    pub async fn create_task<F, Fut>(
        &self,
        future: F,
        name: Option<String>,
    ) -> Result<TaskHandle, String>
    where
        F: FnOnce(crate::task_manager::TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let processor = self.processor.lock().await;
        processor.create_task(future, name).await
    }
}

#[async_trait]
impl FrameProcessorTrait for PipelineSource {
    // Basic processor information
    fn id(&self) -> u64 {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let processor = self.processor.lock().await;
                processor.id()
            })
        })
    }

    fn name(&self) -> &str {
        // We can't delegate this directly due to lifetime issues with async Mutex
        // Return a static string for now - this is a limitation of the current design
        "PipelineSource"
    }

    fn is_started(&self) -> bool {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let processor = self.processor.lock().await;
                processor.is_started()
            })
        })
    }

    fn is_cancelling(&self) -> bool {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let processor = self.processor.lock().await;
                processor.is_cancelling()
            })
        })
    }

    fn can_generate_metrics(&self) -> bool {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let processor = self.processor.lock().await;
                processor.can_generate_metrics()
            })
        })
    }

    // Configuration methods - delegate to inner processor
    fn set_allow_interruptions(&mut self, allow: bool) {
        let processor = self.processor.clone();
        tokio::spawn(async move {
            let mut proc = processor.lock().await;
            proc.set_allow_interruptions(allow);
        });
    }

    fn set_enable_metrics(&mut self, enable: bool) {
        let processor = self.processor.clone();
        tokio::spawn(async move {
            let mut proc = processor.lock().await;
            proc.set_enable_metrics(enable);
        });
    }

    fn set_enable_usage_metrics(&mut self, enable: bool) {
        let processor = self.processor.clone();
        tokio::spawn(async move {
            let mut proc = processor.lock().await;
            proc.set_enable_usage_metrics(enable);
        });
    }

    fn set_report_only_initial_ttfb(&mut self, report: bool) {
        let processor = self.processor.clone();
        tokio::spawn(async move {
            let mut proc = processor.lock().await;
            proc.set_report_only_initial_ttfb(report);
        });
    }

    fn set_clock(&mut self, clock: Arc<dyn BaseClock>) {
        let processor = self.processor.clone();
        tokio::spawn(async move {
            let mut proc = processor.lock().await;
            proc.set_clock(clock);
        });
    }

    fn set_task_manager(&mut self, task_manager: Arc<TaskManager>) {
        let processor = self.processor.clone();
        tokio::spawn(async move {
            let mut proc = processor.lock().await;
            proc.set_task_manager(task_manager);
        });
    }

    // Processor management
    fn add_processor(&mut self, processor: Arc<Mutex<dyn FrameProcessorTrait>>) {
        let self_processor = self.processor.clone();
        tokio::spawn(async move {
            let mut proc = self_processor.lock().await;
            proc.add_processor(processor);
        });
    }

    fn clear_processors(&mut self) {
        let processor = self.processor.clone();
        tokio::spawn(async move {
            let mut proc = processor.lock().await;
            proc.clear_processors();
        });
    }

    fn is_compound_processor(&self) -> bool {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let processor = self.processor.lock().await;
                processor.is_compound_processor()
            })
        })
    }

    fn processor_count(&self) -> usize {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let processor = self.processor.lock().await;
                processor.processor_count()
            })
        })
    }

    fn get_processor(&self, _index: usize) -> Option<&Arc<Mutex<dyn FrameProcessorTrait>>> {
        // Can't return references from async mutex - this is a limitation
        None
    }

    fn processors(&self) -> Vec<Arc<Mutex<dyn FrameProcessorTrait>>> {
        Vec::new()
    }

    fn link(&mut self, next: Arc<Mutex<dyn FrameProcessorTrait>>) {
        let processor = self.processor.clone();
        tokio::spawn(async move {
            let mut proc = processor.lock().await;
            proc.link(next);
        });
    }

    // Interruption strategy
    fn add_interruption_strategy(&mut self, strategy: Arc<dyn BaseInterruptionStrategy>) {
        let processor = self.processor.clone();
        tokio::spawn(async move {
            let mut proc = processor.lock().await;
            proc.add_interruption_strategy(strategy);
        });
    }

    async fn cancel_task(
        &self,
        task: &TaskHandle,
        timeout: Option<std::time::Duration>,
    ) -> Result<(), String> {
        let processor = self.processor.lock().await;
        processor.cancel_task(task, timeout).await
    }

    // Metrics and lifecycle
    async fn get_metrics(&self) -> FrameProcessorMetrics {
        let processor = self.processor.lock().await;
        processor.get_metrics().await
    }

    async fn setup(&mut self, setup: FrameProcessorSetup) -> Result<(), String> {
        let mut processor = self.processor.lock().await;
        processor.setup(setup).await
    }

    async fn setup_all_processors(&self, setup: FrameProcessorSetup) -> Result<(), String> {
        let processor = self.processor.lock().await;
        processor.setup_all_processors(setup).await
    }

    // async fn cleanup_all_processors(&self) -> Result<(), String> {
    //     let processor = self.processor.lock().await;
    //     processor.cleanup_all_processors().await
    // }

    // Frame processing
    async fn push_frame(&self, frame: FrameType, direction: FrameDirection) -> Result<(), String> {
        match direction {
            FrameDirection::Downstream => {
                // Forward downstream frames to the next processor in the pipeline
                let processor = self.processor.lock().await;
                processor.push_frame(frame, direction).await
            }
            FrameDirection::Upstream => {
                // Handle upstream frames through the upstream handler
                (self.upstream_handler)(frame, direction).await
            }
        }
    }

    async fn push_frame_with_callback(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        match direction {
            FrameDirection::Downstream => {
                // Forward downstream frames to the next processor in the pipeline with callback
                let mut processor = self.processor.lock().await;
                processor
                    .push_frame_with_callback(frame, direction, callback)
                    .await
            }
            FrameDirection::Upstream => {
                // Handle upstream frames through the upstream handler
                let result = (self.upstream_handler)(frame, direction).await;

                // Execute callback if provided
                if let Some(callback) = callback {
                    callback();
                }

                result
            }
        }
    }

    async fn queue_frame(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        match direction {
            FrameDirection::Downstream => {
                // Queue downstream frames in the internal processor
                let mut processor = self.processor.lock().await;
                processor.queue_frame(frame, direction, callback).await
            }
            FrameDirection::Upstream => {
                // Handle upstream frames through the upstream handler
                let result = (self.upstream_handler)(frame, direction).await;

                // Execute callback if provided
                if let Some(callback) = callback {
                    callback();
                }

                result
            }
        }
    }

    // Core frame processing method
    async fn process_frame(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
    ) -> Result<(), String> {
        match direction {
            FrameDirection::Downstream => {
                // Process downstream frames through the internal processor
                let mut processor = self.processor.lock().await;
                processor.process_frame(frame, direction).await
            }
            FrameDirection::Upstream => {
                // Handle upstream frames through the upstream handler
                (self.upstream_handler)(frame, direction).await
            }
        }
    }
}

impl std::fmt::Display for PipelineSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PipelineSource")
    }
}
