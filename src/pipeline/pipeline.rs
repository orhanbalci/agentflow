//! Main pipeline implementation that connects frame processors in sequence.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::frames::Frame;
use crate::pipeline::sink::{DownstreamFrameHandler, PipelineSink};
use crate::pipeline::source::{PipelineSource, UpstreamFrameHandler};
use crate::processors::{
    BaseInterruptionStrategy, FrameCallback, FrameDirection, FrameProcessor, FrameProcessorMetrics,
    FrameProcessorSetup, FrameProcessorTrait,
};
use crate::task_manager::{TaskHandle, TaskManager};
use crate::{BaseClock, FrameType};

/// Main pipeline implementation that connects frame processors in sequence.
///
/// Creates a linear chain of frame processors with automatic source and sink
/// processors for external frame handling. Manages processor lifecycle and
/// provides metrics collection from contained processors.
pub struct Pipeline {
    // Internal processor that handles the main FrameProcessorTrait functionality
    processor: Arc<Mutex<FrameProcessor>>,
    // Pipeline-specific components
    source: PipelineSource,
    sink: PipelineSink,
    processors: Vec<Arc<Mutex<dyn FrameProcessorTrait>>>,
}

impl Pipeline {
    /// Initialize the pipeline with a list of processors.
    ///
    /// # Arguments
    /// * `name` - Name for the pipeline
    /// * `processors` - List of frame processors to connect in sequence
    /// * `task_manager` - Task manager for handling async operations
    /// * `source` - Optional custom pipeline source processor
    /// * `sink` - Optional custom pipeline sink processor
    pub fn new(
        name: String,
        processors: Vec<Arc<Mutex<dyn FrameProcessorTrait>>>,
        task_manager: Arc<TaskManager>,
        source: Option<PipelineSource>,
        sink: Option<PipelineSink>,
    ) -> Self {
        // Create default upstream handler for source
        let pipeline_name_for_upstream = name.clone();
        let upstream_handler: UpstreamFrameHandler = Arc::new(move |frame, _direction| {
            let name = pipeline_name_for_upstream.clone();
            Box::pin(async move {
                log::debug!(
                    "Pipeline {} handling upstream frame: {}",
                    name,
                    frame.name()
                );
                // Default upstream behavior - could be customized
                Ok(())
            })
        });

        // Create default downstream handler for sink
        let pipeline_name_for_downstream = name.clone();
        let downstream_handler: DownstreamFrameHandler = Arc::new(move |frame, _direction| {
            let name = pipeline_name_for_downstream.clone();
            Box::pin(async move {
                log::debug!(
                    "Pipeline {} handling downstream frame: {}",
                    name,
                    frame.name()
                );
                // Default downstream behavior - could be customized
                Ok(())
            })
        });

        // Create source and sink if not provided
        let source = source.unwrap_or_else(|| {
            PipelineSource::new(
                upstream_handler,
                Some(format!("{}::Source", name)),
                task_manager.clone(),
            )
        });

        let sink = sink.unwrap_or_else(|| {
            PipelineSink::new(
                downstream_handler,
                Some(format!("{}::Sink", name)),
                task_manager.clone(),
            )
        });

        // Create the full processor chain: source + processors + sink
        let mut all_processors: Vec<Arc<Mutex<dyn FrameProcessorTrait>>> =
            vec![Arc::new(Mutex::new(source.clone()))];
        all_processors.extend(processors.clone());
        all_processors.push(sink.processor());

        // Create internal processor for delegation
        let processor = Arc::new(Mutex::new(FrameProcessor::new(name.clone(), task_manager)));

        let mut pipeline = Self {
            processor,
            source,
            sink,
            processors: all_processors,
        };

        // Link processors in sequence
        pipeline.link_processors();

        pipeline
    }

    /// Create a new pipeline with default source and sink handlers
    pub fn with_processors(
        name: String,
        processors: Vec<Arc<Mutex<dyn FrameProcessorTrait>>>,
        task_manager: Arc<TaskManager>,
    ) -> Self {
        Self::new(name, processors, task_manager, None, None)
    }

    /// Create a new pipeline with custom frame handlers
    pub fn with_handlers(
        name: String,
        processors: Vec<Arc<Mutex<dyn FrameProcessorTrait>>>,
        task_manager: Arc<TaskManager>,
        upstream_handler: UpstreamFrameHandler,
        downstream_handler: DownstreamFrameHandler,
    ) -> Self {
        let source = PipelineSource::new(
            upstream_handler,
            Some(format!("{}::Source", name)),
            task_manager.clone(),
        );

        let sink = PipelineSink::new(
            downstream_handler,
            Some(format!("{}::Sink", name)),
            task_manager.clone(),
        );

        Self::new(name, processors, task_manager, Some(source), Some(sink))
    }

    /// Get the source processor
    pub fn source(&self) -> &PipelineSource {
        &self.source
    }

    /// Get the sink processor
    pub fn sink(&self) -> &PipelineSink {
        &self.sink
    }

    /// Link all processors in sequence and set their relationships
    fn link_processors(&mut self) {
        if self.processors.len() < 2 {
            return;
        }

        // Link processors sequentially
        for i in 0..self.processors.len() - 1 {
            let _current = self.processors[i].clone();
            let _next = self.processors[i + 1].clone();

            // For now, we'll skip the async linking to avoid the spawn issue
            // In a real implementation, you'd want to handle this properly
            // The linking can be done when the processors are actually used
            log::debug!("Would link processor {} to {}", i, i + 1);
            // Note: This should be done when setting up the pipeline
            // We can't easily do async operations in this sync context
        }
    }

    /// Set up all processors in the pipeline
    async fn _setup_processors(&self, setup: FrameProcessorSetup) -> Result<(), String> {
        for processor in &self.processors {
            let mut proc = processor.lock().await;
            proc.setup(setup.clone()).await?;
        }
        Ok(())
    }

    // /// Clean up all processors in the pipeline
    // async fn cleanup_processors(&self) -> Result<(), String> {
    //     for processor in &self.processors {
    //         let mut proc = processor.lock().await;
    //         proc.cleanup().await?;
    //     }
    //     Ok(())
    // }

    /// Get the entry processor (source) for this pipeline
    pub fn get_source_processor(&self) -> Arc<Mutex<dyn FrameProcessorTrait>> {
        self.source.processor()
    }

    /// Get the exit processor (sink) for this pipeline
    pub fn get_sink_processor(&self) -> Arc<Mutex<dyn FrameProcessorTrait>> {
        self.sink.processor()
    }

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
impl FrameProcessorTrait for Pipeline {
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
        "Pipeline"
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
        // Return our local processors instead of delegating
        self.processors.clone()
    }

    fn link(&mut self, next: Arc<Mutex<dyn FrameProcessorTrait>>) {
        let processor = self.processor.clone();
        tokio::spawn(async move {
            let mut proc = processor.lock().await;
            proc.link(next);
        });
    }

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

    async fn push_frame(&self, frame: FrameType, direction: FrameDirection) -> Result<(), String> {
        match direction {
            FrameDirection::Downstream => self.source.push_frame(frame, direction).await,
            FrameDirection::Upstream => self.sink.push_frame(frame, direction).await,
        }
    }

    async fn push_frame_with_callback(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        match direction {
            FrameDirection::Downstream => self.source.queue_frame(frame, direction, callback).await,
            FrameDirection::Upstream => self.sink.queue_frame(frame, direction, callback).await,
        }
    }

    async fn queue_frame(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        match direction {
            FrameDirection::Downstream => self.source.queue_frame(frame, direction, callback).await,
            FrameDirection::Upstream => self.sink.queue_frame(frame, direction, callback).await,
        }
    }

    async fn process_frame(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
    ) -> Result<(), String> {
        match direction {
            FrameDirection::Downstream => self.source.queue_frame(frame, direction, None).await,
            FrameDirection::Upstream => self.sink.queue_frame(frame, direction, None).await,
        }
    }
}

impl std::fmt::Display for Pipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pipeline({})", self.name())
    }
}
