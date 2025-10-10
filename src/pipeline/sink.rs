//! Pipeline sink processor that acts as the exit point for a pipeline.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::processors::{
    BaseInterruptionStrategy, FrameCallback, FrameDirection, FrameProcessor, FrameProcessorMetrics,
    FrameProcessorSetup, FrameProcessorTrait,
};
use crate::task_manager::{TaskHandle, TaskManager};
use crate::{BaseClock, FrameType};

/// Type alias for downstream frame handler function
pub type DownstreamFrameHandler = Arc<
    dyn Fn(
            FrameType,
            FrameDirection,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
        + Send
        + Sync,
>;

/// Sink processor that forwards frames to a downstream handler.
///
/// This processor acts as the exit point for a pipeline, forwarding
/// upstream frames to the previous processor and downstream frames to a
/// provided downstream handler function.
pub struct PipelineSink {
    processor: Arc<Mutex<FrameProcessor>>,
    downstream_handler: DownstreamFrameHandler,
}

impl PipelineSink {
    /// Create a new pipeline sink with the given downstream handler.
    ///
    /// # Arguments
    /// * `downstream_handler` - Function to handle downstream frames
    /// * `name` - Optional name for the sink processor
    /// * `task_manager` - Task manager for handling async operations
    pub fn new(
        downstream_handler: DownstreamFrameHandler,
        name: Option<String>,
        task_manager: Arc<TaskManager>,
    ) -> Self {
        let processor_name = name.unwrap_or_else(|| "PipelineSink".to_string());
        let processor = FrameProcessor::new(processor_name, task_manager);

        Self {
            processor: Arc::new(Mutex::new(processor)),
            downstream_handler,
        }
    }

    /// Get the underlying frame processor
    pub fn processor(&self) -> Arc<Mutex<FrameProcessor>> {
        self.processor.clone()
    }

    // Task management - these need to be async delegated
    async fn _create_task<F, Fut>(
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
impl FrameProcessorTrait for PipelineSink {
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
        "PipelineSink"
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
        // Return a reference to an empty static vector
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
            FrameDirection::Upstream => {
                // Forward upstream frames to the previous processor in the pipeline
                let processor = self.processor.lock().await;
                processor.push_frame(frame, direction).await
            }
            FrameDirection::Downstream => {
                // Handle downstream frames through the downstream handler
                (self.downstream_handler)(frame, direction).await
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
            FrameDirection::Upstream => {
                // Forward upstream frames to the previous processor in the pipeline with callback
                let mut processor = self.processor.lock().await;
                processor
                    .push_frame_with_callback(frame, direction, callback)
                    .await
            }
            FrameDirection::Downstream => {
                // Handle downstream frames through the downstream handler
                let result = (self.downstream_handler)(frame, direction).await;

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
            FrameDirection::Upstream => {
                // Queue upstream frames in the internal processor
                let mut processor = self.processor.lock().await;
                processor.queue_frame(frame, direction, callback).await
            }
            FrameDirection::Downstream => {
                // Handle downstream frames through the downstream handler
                let result = (self.downstream_handler)(frame, direction).await;

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
            FrameDirection::Upstream => {
                // Process upstream frames through the internal processor
                let mut processor = self.processor.lock().await;
                processor.process_frame(frame, direction).await
            }
            FrameDirection::Downstream => {
                // Handle downstream frames through the downstream handler
                (self.downstream_handler)(frame, direction).await
            }
        }
    }
}

impl std::fmt::Display for PipelineSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PipelineSink")
    }
}
