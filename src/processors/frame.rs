// Frame processing pipeline infrastructure for Rust using Tokio
//
// This module provides the core frame processing system that enables building
// audio/video processing pipelines with frame processors, pipeline
// management, and frame flow control mechanisms.

use async_trait::async_trait;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Duration;

use crate::task_manager::{TaskHandle, TaskManager, WatchdogConfig};
use crate::{BaseClock, CancelFrame, ErrorFrame, Frame, FrameType, StartFrame};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameDirection {
    Downstream,
    Upstream,
}

// Metrics and observer types
#[derive(Debug, Clone)]
pub struct MetricsData {
    pub ttfb: Option<Duration>,
    pub processing_time: Option<Duration>,
    pub tokens_used: Option<u64>,
}

#[async_trait]
pub trait Observer: Send + Sync {
    async fn on_process_frame(
        &self,
        processor_name: &str,
        frame: &dyn Frame,
        direction: FrameDirection,
    );
    async fn on_push_frame(
        &self,
        from: &str,
        to: &str,
        frame: &dyn Frame,
        direction: FrameDirection,
    );
}

// Priority queue item for frame processing
struct QueueItem {
    priority: u8,
    counter: u64,
    frame: FrameType,
    direction: FrameDirection,
    callback: Option<FrameCallback>,
}

impl std::fmt::Debug for QueueItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueueItem")
            .field("priority", &self.priority)
            .field("counter", &self.counter)
            .field("frame", &self.frame)
            .field("direction", &self.direction)
            .field("callback", &self.callback.is_some())
            .finish()
    }
}

// Frame callback type
pub type FrameCallback = Box<dyn Fn() + Send + Sync>;

impl PartialEq for QueueItem {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.counter == other.counter
    }
}

impl Eq for QueueItem {}

impl PartialOrd for QueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Lower priority number = higher priority in processing
        other
            .priority
            .cmp(&self.priority)
            .then(self.counter.cmp(&other.counter))
    }
}

// Frame processor setup configuration
#[derive(Clone)]
pub struct FrameProcessorSetup {
    pub observer: Option<Arc<dyn Observer>>,
    pub task_timeout: Option<Duration>,
}

// Main frame processor trait
#[async_trait]
pub trait FrameProcessorTrait: Send + Sync {
    async fn process_frame(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
    ) -> Result<(), String>;
    fn name(&self) -> &str;
    fn can_generate_metrics(&self) -> bool {
        false
    }
}

// Frame processor interface trait
// This trait abstracts all the functionality that can be delegated from other components
#[async_trait]
pub trait FrameProcessorInterface: Send + Sync {
    // Basic processor information
    fn id(&self) -> u64;
    fn name(&self) -> &str;
    fn is_started(&self) -> bool;
    fn is_cancelling(&self) -> bool;

    // Configuration methods
    fn set_allow_interruptions(&mut self, allow: bool);
    fn set_enable_metrics(&mut self, enable: bool);
    fn set_enable_usage_metrics(&mut self, enable: bool);
    fn set_report_only_initial_ttfb(&mut self, report: bool);
    fn set_clock(&mut self, clock: Arc<dyn BaseClock>);
    fn set_task_manager(&mut self, task_manager: Arc<TaskManager>);

    // Processor management
    fn add_processor(&mut self, processor: Arc<Mutex<FrameProcessor>>);
    fn clear_processors(&mut self);
    fn is_compound_processor(&self) -> bool;
    fn processor_count(&self) -> usize;
    fn get_processor(&self, index: usize) -> Option<&Arc<Mutex<FrameProcessor>>>;
    fn processors(&self) -> &Vec<Arc<Mutex<FrameProcessor>>>;
    fn link(&mut self, next: Arc<Mutex<FrameProcessor>>);

    // Interruption strategy
    fn add_interruption_strategy(&mut self, strategy: Arc<dyn BaseInterruptionStrategy>);

    // Task management
    async fn create_task<F, Fut>(
        &self,
        future: F,
        name: Option<String>,
    ) -> Result<TaskHandle, String>
    where
        F: FnOnce(crate::task_manager::TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static;
    async fn cancel_task(
        &self,
        task: &TaskHandle,
        timeout: Option<std::time::Duration>,
    ) -> Result<(), String>;

    // Metrics and lifecycle
    async fn get_metrics(&self) -> FrameProcessorMetrics;
    async fn setup(&mut self, setup: FrameProcessorSetup) -> Result<(), String>;
    async fn setup_all_processors(&self, setup: FrameProcessorSetup) -> Result<(), String>;
    async fn cleanup_all_processors(&self) -> Result<(), String>;

    // Frame processing
    async fn push_frame(&self, frame: FrameType, direction: FrameDirection) -> Result<(), String>;
    async fn push_frame_with_callback(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String>;
}

// Interruption strategy trait
#[async_trait]
pub trait BaseInterruptionStrategy: Send + Sync {
    async fn should_interrupt(&self, frame: &FrameType) -> bool;
}

// Enhanced metrics data structure
#[derive(Debug, Clone)]
pub struct FrameProcessorMetrics {
    pub processor_name: String,
    pub ttfb: Option<Duration>,
    pub processing_time: Option<Duration>,
    pub tokens_used: Option<u64>,
    pub frames_processed: u64,
    pub system_frames_processed: u64,
    pub data_frames_processed: u64,
    pub control_frames_processed: u64,
}

impl FrameProcessorMetrics {
    pub fn new() -> Self {
        Self {
            processor_name: String::new(),
            ttfb: None,
            processing_time: None,
            tokens_used: None,
            frames_processed: 0,
            system_frames_processed: 0,
            data_frames_processed: 0,
            control_frames_processed: 0,
        }
    }

    pub fn set_processor_name(&mut self, name: String) {
        self.processor_name = name;
    }
}

// Frame processor implementation
pub struct FrameProcessor {
    // Basic properties
    id: u64,
    name: String,

    // Pipeline links (_prev and _next in Python)
    prev: Option<Arc<Mutex<FrameProcessor>>>,
    next: Option<Arc<Mutex<FrameProcessor>>>,

    // Configuration (matching Python properties)
    enable_direct_mode: bool,
    allow_interruptions: bool,
    enable_metrics: bool,
    enable_usage_metrics: bool,
    report_only_initial_ttfb: bool,

    // Components (matching Python)
    clock: Option<Arc<dyn BaseClock>>,
    task_manager: Arc<TaskManager>,
    observer: Option<Arc<dyn Observer>>,
    interruption_strategies: Vec<Arc<dyn BaseInterruptionStrategy>>,

    // State (__started and _cancelling in Python)
    started: AtomicBool,
    cancelling: AtomicBool,

    // Frame processing control (__should_block_* in Python)
    should_block_frames: AtomicBool,
    should_block_system_frames: AtomicBool,

    // Events (__input_event and __process_event in Python)
    input_event: Arc<tokio::sync::Notify>,
    process_event: Arc<tokio::sync::Notify>,

    // Tasks (__input_frame_task and __process_frame_task in Python)
    input_frame_task: Option<TaskHandle>,
    process_frame_task: Option<TaskHandle>,

    // Queues and channels
    input_tx: Option<mpsc::UnboundedSender<QueueItem>>,
    process_tx: Option<mpsc::UnboundedSender<QueueItem>>,

    // Counters for priority queue
    high_counter: AtomicU64,
    low_counter: AtomicU64,

    // Metrics (matching Python _metrics)
    metrics: Arc<Mutex<FrameProcessorMetrics>>,

    // Custom processor implementation
    processor_impl: Option<Box<dyn FrameProcessorTrait>>,

    // Sub-processors for compound processors (pipelines, parallel pipelines)
    processors: Vec<Arc<Mutex<FrameProcessor>>>,
}

impl fmt::Display for FrameProcessor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FrameProcessor({}:{})", self.name, self.id)
    }
}

impl FrameProcessor {
    pub fn new(name: String, task_manager: Arc<TaskManager>) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);

        let mut metrics = FrameProcessorMetrics::new();
        metrics.set_processor_name(name.clone());

        Self {
            id,
            name,
            prev: None,
            next: None,
            enable_direct_mode: false,
            allow_interruptions: false,
            enable_metrics: false,
            enable_usage_metrics: false,
            report_only_initial_ttfb: false,
            clock: None,
            task_manager,
            observer: None,
            interruption_strategies: Vec::new(),
            started: AtomicBool::new(false),
            cancelling: AtomicBool::new(false),
            should_block_frames: AtomicBool::new(false),
            should_block_system_frames: AtomicBool::new(false),
            input_event: Arc::new(tokio::sync::Notify::new()),
            process_event: Arc::new(tokio::sync::Notify::new()),
            input_frame_task: None,
            process_frame_task: None,
            input_tx: None,
            process_tx: None,
            high_counter: AtomicU64::new(0),
            low_counter: AtomicU64::new(0),
            metrics: Arc::new(Mutex::new(metrics)),
            processor_impl: None,
            processors: Vec::new(),
        }
    }

    pub fn with_direct_mode(mut self, enable: bool) -> Self {
        self.enable_direct_mode = enable;
        self
    }

    pub fn with_processor_impl(mut self, processor: Box<dyn FrameProcessorTrait>) -> Self {
        self.processor_impl = Some(processor);
        self
    }

    /// Create a new compound processor (pipeline or parallel pipeline)
    pub fn new_compound(
        name: String,
        task_manager: Arc<TaskManager>,
        processors: Vec<Arc<Mutex<FrameProcessor>>>,
    ) -> Self {
        let mut processor = Self::new(name, task_manager);
        processor.processors = processors;
        processor
    }

    /// Create a pipeline processor with automatic linking
    pub fn new_pipeline(
        name: String,
        task_manager: Arc<TaskManager>,
        processors: Vec<Arc<Mutex<FrameProcessor>>>,
    ) -> Self {
        // Link processors in sequence
        for i in 0..processors.len().saturating_sub(1) {
            let current = processors[i].clone();
            let next = processors[i + 1].clone();

            // Note: This is a simplified version. In practice, you'd want to handle
            // the linking more carefully to avoid potential deadlocks
            tokio::spawn(async move {
                let mut current_guard = current.lock().await;
                current_guard.link(next);
            });
        }

        let mut processor = Self::new(name, task_manager);
        processor.processors = processors;
        processor
    }

    // Setters matching Python properties
    pub fn set_clock(&mut self, clock: Arc<dyn BaseClock>) {
        self.clock = Some(clock);
    }

    pub fn set_task_manager(&mut self, task_manager: Arc<TaskManager>) {
        self.task_manager = task_manager;
    }

    pub fn set_observer(&mut self, observer: Arc<dyn Observer>) {
        self.observer = Some(observer);
    }

    pub fn add_interruption_strategy(&mut self, strategy: Arc<dyn BaseInterruptionStrategy>) {
        self.interruption_strategies.push(strategy);
    }

    pub fn set_allow_interruptions(&mut self, allow: bool) {
        self.allow_interruptions = allow;
    }

    pub fn set_enable_metrics(&mut self, enable: bool) {
        self.enable_metrics = enable;
    }

    pub fn set_enable_usage_metrics(&mut self, enable: bool) {
        self.enable_usage_metrics = enable;
    }

    pub fn set_report_only_initial_ttfb(&mut self, report: bool) {
        self.report_only_initial_ttfb = report;
    }

    // Getters matching Python properties
    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::Relaxed)
    }

    pub fn is_cancelling(&self) -> bool {
        self.cancelling.load(Ordering::Relaxed)
    }

    pub async fn get_metrics(&self) -> FrameProcessorMetrics {
        self.metrics.lock().await.clone()
    }

    pub fn processors(&self) -> &Vec<Arc<Mutex<FrameProcessor>>> {
        &self.processors
    }

    /// Add a sub-processor to this processor (for compound processors)
    pub fn add_processor(&mut self, processor: Arc<Mutex<FrameProcessor>>) {
        self.processors.push(processor);
    }

    /// Remove all sub-processors
    pub fn clear_processors(&mut self) {
        self.processors.clear();
    }

    /// Check if this is a compound processor (has sub-processors)
    pub fn is_compound_processor(&self) -> bool {
        !self.processors.is_empty()
    }

    /// Get the number of sub-processors
    pub fn processor_count(&self) -> usize {
        self.processors.len()
    }

    /// Get a specific sub-processor by index
    pub fn get_processor(&self, index: usize) -> Option<&Arc<Mutex<FrameProcessor>>> {
        self.processors.get(index)
    }

    /// Setup all sub-processors with the same configuration
    pub async fn setup_all_processors(&self, setup: FrameProcessorSetup) -> Result<(), String> {
        for processor in &self.processors {
            let mut proc_guard = processor.lock().await;
            proc_guard.setup(setup.clone()).await?;
        }
        Ok(())
    }

    /// Cleanup all sub-processors
    pub async fn cleanup_all_processors(&self) -> Result<(), String> {
        for processor in &self.processors {
            let mut proc_guard = processor.lock().await;
            proc_guard.cleanup().await?;
        }
        Ok(())
    }

    /// Return the list of entry processors for this processor.
    ///
    /// Entry processors are the first processors in a compound processor
    /// (e.g. pipelines, parallel pipelines). Note that pipelines can also be an
    /// entry processor as pipelines are processors themselves. Non-compound
    /// processors will simply return an empty list.
    ///
    /// Returns:
    ///     The list of entry processors.
    pub fn entry_processors(&self) -> Vec<Arc<Mutex<FrameProcessor>>> {
        if self.processors.is_empty() {
            // Non-compound processors return empty list
            Vec::new()
        } else {
            // For compound processors, the entry processor is typically the first one
            // For pipelines, this would be the first processor in the chain
            // For parallel pipelines, this could be all processors that don't have predecessors
            vec![self.processors[0].clone()]
        }
    }

    pub fn next(&self) -> Option<Arc<Mutex<FrameProcessor>>> {
        self.next.clone()
    }

    pub fn previous(&self) -> Option<Arc<Mutex<FrameProcessor>>> {
        self.prev.clone()
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn interruptions_allowed(&self) -> bool {
        self.allow_interruptions
    }

    pub fn metrics_enabled(&self) -> bool {
        self.enable_metrics
    }

    pub fn usage_metrics_enabled(&self) -> bool {
        self.enable_usage_metrics
    }

    pub fn report_only_initial_ttfb(&self) -> bool {
        self.report_only_initial_ttfb
    }

    pub fn interruption_strategies(&self) -> &Vec<Arc<dyn BaseInterruptionStrategy>> {
        &self.interruption_strategies
    }

    pub fn can_generate_metrics(&self) -> bool {
        false
    }

    /// Check if this processor has a TaskManager configured
    pub fn has_task_manager(&self) -> bool {
        true // TaskManager is now mandatory
    }

    /// Get a reference to the TaskManager
    pub fn task_manager(&self) -> &Arc<TaskManager> {
        &self.task_manager
    }

    /// Create a new task managed by this processor.
    ///
    /// Args:
    ///     future: The future to run in the task.
    ///     name: Optional name for the task.
    ///
    /// Returns:
    ///     The created task handle.
    pub async fn create_task<F, Fut>(
        &self,
        future: F,
        name: Option<String>,
    ) -> Result<TaskHandle, String>
    where
        F: FnOnce(crate::task_manager::TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let task_name = if let Some(name) = name {
            format!("{}::{}", self, name)
        } else {
            format!("{}::task", self)
        };

        self.task_manager
            .create_task(task_name, future, None)
            .await
            .map_err(|e| format!("Failed to create task: {:?}", e))
    }

    /// Cancel a task managed by this processor.
    ///
    /// Args:
    ///     task: The task to cancel.
    ///     timeout: Optional timeout for task cancellation.
    pub async fn cancel_task(
        &self,
        task: &TaskHandle,
        timeout: Option<Duration>,
    ) -> Result<(), String> {
        self.task_manager
            .cancel_task(task, timeout)
            .await
            .map_err(|e| format!("Failed to cancel task: {:?}", e))
    }

    /// Setup method for processors
    pub async fn setup_with_task_manager(
        &mut self,
        setup: FrameProcessorSetup,
    ) -> Result<(), String> {
        self.observer = setup.observer;

        if !self.enable_direct_mode {
            self.create_input_task().await?;
        }

        Ok(())
    }

    pub async fn setup(&mut self, setup: FrameProcessorSetup) -> Result<(), String> {
        self.observer = setup.observer;

        if !self.enable_direct_mode {
            self.create_input_task().await?;
        }

        Ok(())
    }

    pub async fn cleanup(&mut self) -> Result<(), String> {
        self.cancel_input_task().await;
        self.cancel_process_task().await;
        Ok(())
    }

    pub fn link(&mut self, next: Arc<Mutex<FrameProcessor>>) {
        self.next = Some(next.clone());
        // Set the previous link on the next processor
        // Note: This would require more complex handling in a real implementation
        // to avoid deadlocks when locking multiple processors
    }

    pub async fn queue_frame(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        // If we are cancelling we don't want to process any other frame.
        if self.cancelling.load(Ordering::Relaxed) {
            return Ok(());
        }

        if self.enable_direct_mode {
            // For direct mode, process frame directly and call callback
            log::debug!("{}: Processing frame {} in direct mode", self, frame.name());
            self.process_frame_direct(frame, direction, callback).await
        } else {
            let priority = if frame.is_system_frame() { 1 } else { 2 };
            let counter = if frame.is_system_frame() {
                self.high_counter.fetch_add(1, Ordering::Relaxed)
            } else {
                self.low_counter.fetch_add(1, Ordering::Relaxed)
            };

            let item = QueueItem {
                priority,
                counter,
                frame,
                direction,
                callback,
            };

            if let Some(tx) = &self.input_tx {
                tx.send(item)
                    .map_err(|e| format!("Failed to queue frame: {}", e))?;
            }

            Ok(())
        }
    }

    /// Convenience method to queue a frame without callback (matching Python's default behavior)
    pub async fn queue_frame_simple(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
    ) -> Result<(), String> {
        self.queue_frame(frame, direction, None).await
    }

    /// Process a frame directly in direct mode with callback support
    /// This corresponds to Python's __process_frame method
    async fn process_frame_direct(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        // Process the frame.
        match self.process_frame_impl(frame.clone(), direction).await {
            Ok(_) => {
                // If this frame has an associated callback, call it now.
                if let Some(callback) = callback {
                    callback();
                }
                Ok(())
            }
            Err(e) => {
                log::error!("{}: error processing frame: {}", self, e);
                // Note: In a real implementation, we might want to push the error
                // but for now we'll just log it to avoid recursion
                Err(e)
            }
        }
    }

    /// Internal frame processing implementation
    /// This corresponds to Python's process_frame method
    async fn process_frame_impl(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
    ) -> Result<(), String> {
        // Notify observer with timestamp if available
        if let Some(observer) = &self.observer {
            let timestamp = if let Some(clock) = &self.clock {
                clock.get_time().unwrap_or(0)
            } else {
                0
            };

            // Create FrameProcessed data structure (simplified version)
            log::debug!(
                "{}: Observer notification - frame: {}, direction: {:?}, timestamp: {}",
                self.name,
                frame.name(),
                direction,
                timestamp
            );

            observer
                .on_process_frame(&self.name, &frame, direction)
                .await;
        }

        // Handle frame types (matching Python's isinstance checks)
        match &frame {
            FrameType::Start(start_frame) => {
                // Call handle_start to properly initialize processor state
                self.handle_start(start_frame.clone()).await?;
            }
            FrameType::Cancel(cancel_frame) => {
                // Call handle_cancel to properly handle cancellation
                self.handle_cancel(cancel_frame.clone()).await?;
            }
            FrameType::Error(error_frame) => {
                log::debug!(
                    "{}: Processing ErrorFrame: {}",
                    self.name,
                    error_frame.error
                );
                // Handle error frame logic here
            }
            // TODO: Add handling for StartInterruptionFrame when it's added to FrameType enum
            // For now, this would be handled in the default case:
            // FrameType::StartInterruption(_) => {
            //     self.start_interruption().await?;
            //     self.stop_all_metrics().await?;
            // }
            // TODO: Add handling for other frame types like:
            // - StopInterruptionFrame (resume after interruption)
            // - FrameProcessorPauseFrame/FrameProcessorPauseUrgentFrame (pause processing)
            // - FrameProcessorResumeFrame/FrameProcessorResumeUrgentFrame (resume processing)
            // These would correspond to additional frame types in the FrameType enum
            _ => {
                log::debug!("{}: Processing frame {}", self.name, frame.name());

                // Check if this is a StartInterruptionFrame by name (until proper enum variant exists)
                if frame.name() == "StartInterruptionFrame" {
                    log::debug!("{}: Processing StartInterruptionFrame", self.name);
                    self.start_interruption().await?;
                    self.stop_all_metrics().await?;
                } else {
                    // Handle other frame types or delegate to custom processor
                    if let Some(_processor_impl) = &self.processor_impl {
                        // Note: This would require making processor_impl mutable or using interior mutability
                        // For now, we'll just log that we would call the custom processor
                        log::debug!(
                            "{}: Would call custom processor for frame {}",
                            self.name,
                            frame.name()
                        );
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn pause_processing_frames(&self) {
        log::trace!("{}: pausing frame processing", self);
        self.should_block_frames.store(true, Ordering::Relaxed);
    }

    pub async fn pause_processing_system_frames(&self) {
        log::trace!("{}: pausing system frame processing", self);
        self.should_block_system_frames
            .store(true, Ordering::Relaxed);
    }

    pub async fn resume_processing_frames(&self) {
        log::trace!("{}: resuming frame processing", self);
        self.process_event.notify_one();
    }

    pub async fn resume_processing_system_frames(&self) {
        log::trace!("{}: resuming system frame processing", self);
        self.input_event.notify_one();
    }

    pub async fn push_frame(
        &self,
        frame: FrameType,
        direction: FrameDirection,
    ) -> Result<(), String> {
        if !self.check_started(&frame) {
            return Ok(());
        }

        self.internal_push_frame(frame, direction).await
    }

    pub async fn push_frame_with_callback(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        if !self.check_started(&frame) {
            return Ok(());
        }

        self.queue_frame(frame, direction, callback).await
    }

    pub async fn push_error(&self, error: ErrorFrame) -> Result<(), String> {
        self.push_frame(FrameType::Error(error), FrameDirection::Upstream)
            .await
    }

    async fn handle_start(&mut self, frame: StartFrame) -> Result<(), String> {
        // Handle the start frame to initialize processor state.
        self.started.store(true, Ordering::Relaxed);
        self.allow_interruptions = frame.allow_interruptions;
        self.enable_metrics = frame.enable_metrics;

        // Note: These fields are not yet available in the current StartFrame implementation
        // but should be added to match the Python version:
        // self.enable_usage_metrics = frame.enable_usage_metrics;
        // self.report_only_initial_ttfb = frame.report_only_initial_ttfb;
        // self.interruption_strategies = frame.interruption_strategies;

        if !self.enable_direct_mode {
            self.create_process_task().await?;
        }

        Ok(())
    }

    async fn handle_cancel(&mut self, _frame: CancelFrame) -> Result<(), String> {
        self.cancelling.store(true, Ordering::Relaxed);
        self.cancel_process_task().await;
        Ok(())
    }

    pub async fn start_interruption(&mut self) -> Result<(), String> {
        // Start handling an interruption by cancelling current tasks.
        // This matches Python's _start_interruption method
        log::debug!("{}: Starting interruption", self.name);

        // Cancel the process task. This will stop processing queued frames.
        self.cancel_process_task().await;

        // Create a new process queue and task.
        if !self.enable_direct_mode {
            self.create_process_task().await?;
        }

        Ok(())
    }

    async fn internal_push_frame(
        &self,
        frame: FrameType,
        direction: FrameDirection,
    ) -> Result<(), String> {
        match direction {
            FrameDirection::Downstream => {
                if let Some(next) = &self.next {
                    log::trace!("Pushing {} from {} downstream", frame.name(), self);
                    if let Some(observer) = &self.observer {
                        let next_guard = next.lock().await;
                        observer
                            .on_push_frame(&self.name, &next_guard.name, &frame, direction)
                            .await;
                        drop(next_guard);
                    }
                    let mut next_guard = next.lock().await;
                    next_guard.queue_frame(frame, direction, None).await?;
                }
            }
            FrameDirection::Upstream => {
                if let Some(prev) = &self.prev {
                    log::trace!("Pushing {} from {} upstream", frame.name(), self);
                    if let Some(observer) = &self.observer {
                        let prev_guard = prev.lock().await;
                        observer
                            .on_push_frame(&self.name, &prev_guard.name, &frame, direction)
                            .await;
                        drop(prev_guard);
                    }
                    let mut prev_guard = prev.lock().await;
                    prev_guard.queue_frame(frame, direction, None).await?;
                }
            }
        }
        Ok(())
    }

    fn check_started(&self, frame: &dyn Frame) -> bool {
        let started = self.started.load(Ordering::Relaxed);
        if !started {
            log::error!(
                "{}: Trying to process {} but StartFrame not received yet",
                self,
                frame.name()
            );
        }
        started
    }

    async fn create_input_task(&mut self) -> Result<(), String> {
        if self.input_frame_task.is_some() {
            return Ok(());
        }

        let (tx, rx) = mpsc::unbounded_channel::<QueueItem>();
        self.input_tx = Some(tx);

        let name = self.name.clone();
        let observer = self.observer.clone();
        let should_block_system_frames = Arc::new(AtomicBool::new(false));
        let should_block_system_frames_clone = should_block_system_frames.clone();
        let input_event = self.input_event.clone();
        let process_tx_clone = self.process_tx.clone();

        let task_handle = self.task_manager
            .create_task(
                format!("{}-input", name),
                |ctx| {
                        Box::pin(async move {
                            let mut rx = rx;
                            let mut priority_queue = std::collections::BinaryHeap::new();

                            loop {
                                // Reset watchdog periodically
                                ctx.reset_watchdog();

                                // Check if we should block system frames
                                if should_block_system_frames_clone.load(Ordering::Relaxed) {
                                    log::trace!("{}: system frame processing paused", name);
                                    input_event.notified().await;
                                    should_block_system_frames_clone.store(false, Ordering::Relaxed);
                                    log::trace!("{}: system frame processing resumed", name);
                                }

                                // Get next item from channel or process queued items
                                tokio::select! {
                                    item = rx.recv() => {
                                        if let Some(item) = item {
                                            priority_queue.push(item);
                                        } else {
                                            break; // Channel closed
                                        }
                                    }
                                    _ = tokio::time::sleep(Duration::from_millis(1)), if !priority_queue.is_empty() => {
                                        // Process queued items
                                    }
                                }

                                // Process items from priority queue
                                while let Some(item) = priority_queue.pop() {
                                    ctx.reset_watchdog();

                                    if item.frame.is_system_frame() {
                                        // Process system frame immediately
                                        log::trace!("Processing system frame: {}", item.frame.name());

                                        // Notify observer if available
                                        if let Some(observer) = &observer {
                                            observer.on_process_frame(&name, &item.frame, item.direction).await;
                                        }

                                        // Handle built-in frame types
                                        match &item.frame {
                                            FrameType::Start(_) => {
                                                log::debug!("{}: Processing StartFrame in task", name);
                                            }
                                            FrameType::Cancel(_) => {
                                                log::debug!("{}: Processing CancelFrame in task", name);
                                            }
                                            FrameType::Error(error_frame) => {
                                                log::debug!("{}: Processing ErrorFrame in task: {}", name, error_frame.error);
                                            }
                                            _ => {
                                                log::debug!("{}: Processing frame {} in task", name, item.frame.name());
                                            }
                                        }

                                        // Call callback if provided
                                        if let Some(callback) = item.callback {
                                            callback();
                                        }
                                    } else {
                                        // Queue non-system frame for process task
                                        if let Some(process_tx) = &process_tx_clone {
                                            if let Err(_) = process_tx.send(item) {
                                                log::error!("Failed to send frame to process task");
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        })
                    },
                    Some(WatchdogConfig {
                        enable_timers: true,
                        enable_logging: false,
                        timeout: Some(Duration::from_secs(5)),
                    }),
                )
                .await
                .map_err(|e| format!("Failed to create input task: {:?}", e))?;

        self.input_frame_task = Some(task_handle);

        Ok(())
    }

    async fn cancel_input_task(&mut self) {
        if let Some(task_handle) = self.input_frame_task.take() {
            let _ = self
                .task_manager
                .cancel_task(&task_handle, Some(Duration::from_secs(5)))
                .await;
        }
        self.input_tx = None;
    }

    async fn create_process_task(&mut self) -> Result<(), String> {
        if self.process_frame_task.is_some() {
            return Ok(());
        }

        let (tx, rx) = mpsc::unbounded_channel::<QueueItem>();
        self.process_tx = Some(tx);

        let name = self.name.clone();
        let observer = self.observer.clone();
        let should_block_frames = Arc::new(AtomicBool::new(false));
        let should_block_frames_clone = should_block_frames.clone();
        let process_event = self.process_event.clone();

        let task_handle = self
            .task_manager
            .create_task(
                format!("{}-process", name),
                |ctx| {
                    Box::pin(async move {
                        let mut rx = rx;

                        while let Some(item) = rx.recv().await {
                            // Reset watchdog for each frame
                            ctx.reset_watchdog();

                            // Check if we should block frame processing
                            if should_block_frames_clone.load(Ordering::Relaxed) {
                                log::trace!("{}: frame processing paused", name);
                                process_event.notified().await;
                                should_block_frames_clone.store(false, Ordering::Relaxed);
                                log::trace!("{}: frame processing resumed", name);
                            }

                            // Process the frame
                            log::trace!("Processing frame: {}", item.frame.name());

                            // Notify observer if available
                            if let Some(observer) = &observer {
                                observer
                                    .on_process_frame(&name, &item.frame, item.direction)
                                    .await;
                            }

                            // Handle built-in frame types
                            match &item.frame {
                                FrameType::Start(_) => {
                                    log::debug!("{}: Processing StartFrame in process task", name);
                                }
                                FrameType::Cancel(_) => {
                                    log::debug!("{}: Processing CancelFrame in process task", name);
                                }
                                FrameType::Error(error_frame) => {
                                    log::debug!(
                                        "{}: Processing ErrorFrame in process task: {}",
                                        name,
                                        error_frame.error
                                    );
                                }
                                _ => {
                                    log::debug!(
                                        "{}: Processing frame {} in process task",
                                        name,
                                        item.frame.name()
                                    );
                                }
                            }

                            // Call callback if provided
                            if let Some(callback) = item.callback {
                                callback();
                            }
                        }
                    })
                },
                Some(WatchdogConfig {
                    enable_timers: true,
                    enable_logging: false,
                    timeout: Some(Duration::from_secs(5)),
                }),
            )
            .await
            .map_err(|e| format!("Failed to create process task: {:?}", e))?;

        self.process_frame_task = Some(task_handle);

        Ok(())
    }

    async fn cancel_process_task(&mut self) {
        if let Some(task_handle) = self.process_frame_task.take() {
            let _ = self
                .task_manager
                .cancel_task(&task_handle, Some(Duration::from_secs(5)))
                .await;
        }
        self.process_tx = None;
    }

    /// Stop all active metrics collection
    /// This matches Python's stop_all_metrics method
    pub async fn stop_all_metrics(&mut self) -> Result<(), String> {
        log::debug!("{}: Stopping all metrics", self.name);
        self.stop_ttfb_metrics().await?;
        self.stop_processing_metrics().await?;
        Ok(())
    }

    /// Stop TTFB (Time To First Byte) metrics collection
    pub async fn stop_ttfb_metrics(&mut self) -> Result<(), String> {
        log::debug!("{}: Stopping TTFB metrics", self.name);
        // TODO: Implement TTFB metrics stopping logic
        // This would typically reset TTFB measurement state
        Ok(())
    }

    /// Stop processing metrics collection
    pub async fn stop_processing_metrics(&mut self) -> Result<(), String> {
        log::debug!("{}: Stopping processing metrics", self.name);
        // TODO: Implement processing metrics stopping logic
        // This would typically reset processing time measurement state
        Ok(())
    }
}

// Implement the FrameProcessorInterface trait for FrameProcessor
#[async_trait]
impl FrameProcessorInterface for FrameProcessor {
    // Basic processor information
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_started(&self) -> bool {
        self.started.load(Ordering::Relaxed)
    }

    fn is_cancelling(&self) -> bool {
        self.cancelling.load(Ordering::Relaxed)
    }

    // Configuration methods
    fn set_allow_interruptions(&mut self, allow: bool) {
        self.allow_interruptions = allow;
    }

    fn set_enable_metrics(&mut self, enable: bool) {
        self.enable_metrics = enable;
    }

    fn set_enable_usage_metrics(&mut self, enable: bool) {
        self.enable_usage_metrics = enable;
    }

    fn set_report_only_initial_ttfb(&mut self, report: bool) {
        self.report_only_initial_ttfb = report;
    }

    fn set_clock(&mut self, clock: Arc<dyn BaseClock>) {
        self.clock = Some(clock);
    }

    fn set_task_manager(&mut self, task_manager: Arc<TaskManager>) {
        self.task_manager = task_manager;
    }

    // Processor management
    fn add_processor(&mut self, processor: Arc<Mutex<FrameProcessor>>) {
        self.processors.push(processor);
    }

    fn clear_processors(&mut self) {
        self.processors.clear();
    }

    fn is_compound_processor(&self) -> bool {
        !self.processors.is_empty()
    }

    fn processor_count(&self) -> usize {
        self.processors.len()
    }

    fn get_processor(&self, index: usize) -> Option<&Arc<Mutex<FrameProcessor>>> {
        self.processors.get(index)
    }

    fn processors(&self) -> &Vec<Arc<Mutex<FrameProcessor>>> {
        &self.processors
    }

    fn link(&mut self, next: Arc<Mutex<FrameProcessor>>) {
        self.next = Some(next);
    }

    // Interruption strategy
    fn add_interruption_strategy(&mut self, strategy: Arc<dyn BaseInterruptionStrategy>) {
        self.interruption_strategies.push(strategy);
    }

    // Task management
    async fn create_task<F, Fut>(
        &self,
        future: F,
        name: Option<String>,
    ) -> Result<TaskHandle, String>
    where
        F: FnOnce(crate::task_manager::TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let task_name = if let Some(name) = name {
            format!("{}::{}", self, name)
        } else {
            format!("{}::task", self)
        };

        self.task_manager
            .create_task(task_name, future, None)
            .await
            .map_err(|e| format!("Failed to create task: {:?}", e))
    }

    async fn cancel_task(
        &self,
        task: &TaskHandle,
        timeout: Option<Duration>,
    ) -> Result<(), String> {
        self.task_manager
            .cancel_task(task, timeout)
            .await
            .map_err(|e| format!("Failed to cancel task: {:?}", e))
    }

    // Metrics and lifecycle
    async fn get_metrics(&self) -> FrameProcessorMetrics {
        self.metrics.lock().await.clone()
    }

    async fn setup(&mut self, setup: FrameProcessorSetup) -> Result<(), String> {
        self.observer = setup.observer;

        if !self.enable_direct_mode {
            self.create_input_task().await?;
        }

        Ok(())
    }

    async fn setup_all_processors(&self, setup: FrameProcessorSetup) -> Result<(), String> {
        for processor in &self.processors {
            let mut proc_guard = processor.lock().await;
            proc_guard.setup(setup.clone()).await?;
        }
        Ok(())
    }

    async fn cleanup_all_processors(&self) -> Result<(), String> {
        for processor in &self.processors {
            let mut proc_guard = processor.lock().await;
            proc_guard.cleanup().await?;
        }
        Ok(())
    }

    // Frame processing
    async fn push_frame(&self, frame: FrameType, direction: FrameDirection) -> Result<(), String> {
        if !self.check_started(&frame) {
            return Ok(());
        }

        self.internal_push_frame(frame, direction).await
    }

    async fn push_frame_with_callback(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        if !self.check_started(&frame) {
            return Ok(());
        }

        self.queue_frame(frame, direction, callback).await
    }
}

// Implement the FrameProcessorInterface trait for Arc<Mutex<FrameProcessor>>
// This allows direct usage of the interface without manually locking
#[async_trait]
impl FrameProcessorInterface for Arc<Mutex<FrameProcessor>> {
    // Basic processor information
    fn id(&self) -> u64 {
        // For sync methods with tokio::Mutex, we need to block on the async operation
        let arc_clone = self.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let processor = arc_clone.lock().await;
                processor.id()
            })
        })
    }

    fn name(&self) -> &str {
        // This can't work with async mutex as we can't return a reference
        // This is a limitation of the current design
        "FrameProcessor"
    }

    fn is_started(&self) -> bool {
        let arc_clone = self.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let processor = arc_clone.lock().await;
                processor.is_started()
            })
        })
    }

    fn is_cancelling(&self) -> bool {
        let arc_clone = self.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let processor = arc_clone.lock().await;
                processor.is_cancelling()
            })
        })
    }

    // Configuration methods
    fn set_allow_interruptions(&mut self, allow: bool) {
        let arc_clone = self.clone();
        tokio::spawn(async move {
            let mut processor = arc_clone.lock().await;
            processor.set_allow_interruptions(allow);
        });
    }

    fn set_enable_metrics(&mut self, enable: bool) {
        let arc_clone = self.clone();
        tokio::spawn(async move {
            let mut processor = arc_clone.lock().await;
            processor.set_enable_metrics(enable);
        });
    }

    fn set_enable_usage_metrics(&mut self, enable: bool) {
        let arc_clone = self.clone();
        tokio::spawn(async move {
            let mut processor = arc_clone.lock().await;
            processor.set_enable_usage_metrics(enable);
        });
    }

    fn set_report_only_initial_ttfb(&mut self, report: bool) {
        let arc_clone = self.clone();
        tokio::spawn(async move {
            let mut processor = arc_clone.lock().await;
            processor.set_report_only_initial_ttfb(report);
        });
    }

    fn set_clock(&mut self, clock: Arc<dyn BaseClock>) {
        let arc_clone = self.clone();
        tokio::spawn(async move {
            let mut processor = arc_clone.lock().await;
            processor.set_clock(clock);
        });
    }

    fn set_task_manager(&mut self, task_manager: Arc<TaskManager>) {
        let arc_clone = self.clone();
        tokio::spawn(async move {
            let mut processor = arc_clone.lock().await;
            processor.set_task_manager(task_manager);
        });
    }

    // Processor management
    fn add_processor(&mut self, processor: Arc<Mutex<FrameProcessor>>) {
        let arc_clone = self.clone();
        tokio::spawn(async move {
            let mut fp = arc_clone.lock().await;
            fp.add_processor(processor);
        });
    }

    fn clear_processors(&mut self) {
        let arc_clone = self.clone();
        tokio::spawn(async move {
            let mut fp = arc_clone.lock().await;
            fp.clear_processors();
        });
    }

    fn is_compound_processor(&self) -> bool {
        let arc_clone = self.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let processor = arc_clone.lock().await;
                processor.is_compound_processor()
            })
        })
    }

    fn processor_count(&self) -> usize {
        let arc_clone = self.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let processor = arc_clone.lock().await;
                processor.processor_count()
            })
        })
    }

    fn get_processor(&self, _index: usize) -> Option<&Arc<Mutex<FrameProcessor>>> {
        // This can't work with async mutex as we can't return a reference
        // The lifetime of the returned reference would be tied to the lock guard
        // which would be dropped immediately
        None
    }

    fn processors(&self) -> &Vec<Arc<Mutex<FrameProcessor>>> {
        // Same issue as get_processor - can't return references from async mutex
        static EMPTY: Vec<Arc<Mutex<FrameProcessor>>> = Vec::new();
        &EMPTY
    }

    fn link(&mut self, next: Arc<Mutex<FrameProcessor>>) {
        let arc_clone = self.clone();
        tokio::spawn(async move {
            let mut processor = arc_clone.lock().await;
            processor.link(next);
        });
    }

    // Interruption strategy
    fn add_interruption_strategy(&mut self, strategy: Arc<dyn BaseInterruptionStrategy>) {
        let arc_clone = self.clone();
        tokio::spawn(async move {
            let mut processor = arc_clone.lock().await;
            processor.add_interruption_strategy(strategy);
        });
    }

    // Task management
    async fn create_task<F, Fut>(
        &self,
        future: F,
        name: Option<String>,
    ) -> Result<TaskHandle, String>
    where
        F: FnOnce(crate::task_manager::TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let processor = self.lock().await;
        processor.create_task(future, name).await
    }

    async fn cancel_task(
        &self,
        task: &TaskHandle,
        timeout: Option<Duration>,
    ) -> Result<(), String> {
        let processor = self.lock().await;
        processor.cancel_task(task, timeout).await
    }

    // Metrics and lifecycle
    async fn get_metrics(&self) -> FrameProcessorMetrics {
        let processor = self.lock().await;
        processor.get_metrics().await
    }

    async fn setup(&mut self, setup: FrameProcessorSetup) -> Result<(), String> {
        let mut processor = self.lock().await;
        processor.setup(setup).await
    }

    async fn setup_all_processors(&self, setup: FrameProcessorSetup) -> Result<(), String> {
        let processor = self.lock().await;
        processor.setup_all_processors(setup).await
    }

    async fn cleanup_all_processors(&self) -> Result<(), String> {
        let processor = self.lock().await;
        processor.cleanup_all_processors().await
    }

    // Frame processing
    async fn push_frame(&self, frame: FrameType, direction: FrameDirection) -> Result<(), String> {
        let processor = self.lock().await;
        processor.push_frame(frame, direction).await
    }

    async fn push_frame_with_callback(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        let mut processor = self.lock().await;
        processor
            .push_frame_with_callback(frame, direction, callback)
            .await
    }
}
