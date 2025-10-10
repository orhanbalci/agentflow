// Basic Task Manager with Watchdog Pattern using Tokio

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TaskManagerConfig {
    pub enable_watchdog_timers: bool,
    pub enable_watchdog_logging: bool,
    pub default_watchdog_timeout: Duration,
}

impl Default for TaskManagerConfig {
    fn default() -> Self {
        Self {
            enable_watchdog_timers: false,
            enable_watchdog_logging: false,
            default_watchdog_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug)]
pub struct TaskData {
    pub id: Uuid,
    pub name: String,
    pub created_at: Instant,
    pub enable_watchdog_logging: bool,
    pub enable_watchdog_timers: bool,
    pub watchdog_timeout: Duration,
    pub watchdog_tx: Option<broadcast::Sender<()>>,
}

pub struct TaskManager {
    tasks: Arc<RwLock<HashMap<Uuid, TaskData>>>,
    task_handles: Arc<RwLock<HashMap<Uuid, JoinHandle<()>>>>,
    config: TaskManagerConfig,
    shutdown_tx: broadcast::Sender<()>,
}

impl TaskManager {
    pub fn new(config: TaskManagerConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(16);

        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            task_handles: Arc::new(RwLock::new(HashMap::new())),
            config,
            shutdown_tx,
        }
    }

    pub async fn create_task<F, Fut>(
        &self,
        name: String,
        future: F,
        watchdog_config: Option<WatchdogConfig>,
    ) -> Result<TaskHandle, TaskError>
    where
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let task_id = Uuid::new_v4();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // Setup watchdog if enabled
        let (watchdog_tx, watchdog_rx) = if watchdog_config
            .as_ref()
            .map(|c| c.enable_timers)
            .unwrap_or(self.config.enable_watchdog_timers)
        {
            let (tx, rx) = broadcast::channel(16);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let task_name = name.clone();
        let watchdog_timeout = watchdog_config
            .as_ref()
            .and_then(|c| c.timeout)
            .unwrap_or(self.config.default_watchdog_timeout);

        // Create task context
        let context = TaskContext {
            id: task_id,
            name: task_name.clone(),
            watchdog_tx: watchdog_tx.clone(),
        };

        // Spawn the main task
        let handle = tokio::spawn({
            let task_name_for_spawn = task_name.clone();
            async move {
                info!("Task '{}' ({}) started", task_name_for_spawn, task_id);

                tokio::select! {
                    _ = future(context) => {
                        info!("Task '{}' ({}) completed normally", task_name_for_spawn, task_id);
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Task '{}' ({}) shutting down due to global shutdown", task_name_for_spawn, task_id);
                    }
                }
            }
        });

        // Spawn watchdog task if enabled
        if let Some(watchdog_rx) = watchdog_rx {
            let watchdog_name = format!("watchdog-{}", task_name);
            let enable_logging = watchdog_config
                .as_ref()
                .map(|c| c.enable_logging)
                .unwrap_or(self.config.enable_watchdog_logging);

            tokio::spawn(Self::watchdog_monitor(
                watchdog_name,
                task_id,
                watchdog_rx,
                watchdog_timeout,
                enable_logging,
            ));
        }

        // Store task data
        let task_data = TaskData {
            id: task_id,
            name: name.clone(),
            created_at: Instant::now(),
            enable_watchdog_logging: watchdog_config
                .as_ref()
                .map(|c| c.enable_logging)
                .unwrap_or(self.config.enable_watchdog_logging),
            enable_watchdog_timers: watchdog_config
                .as_ref()
                .map(|c| c.enable_timers)
                .unwrap_or(self.config.enable_watchdog_timers),
            watchdog_timeout,
            watchdog_tx,
        };

        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id, task_data);
        }

        {
            let mut task_handles = self.task_handles.write().await;
            task_handles.insert(task_id, handle);
        }

        // Setup cleanup when task completes
        let tasks_cleanup = Arc::clone(&self.tasks);
        let handles_cleanup = Arc::clone(&self.task_handles);
        let cleanup_task_id = task_id;
        let cleanup_name = name.clone();
        tokio::spawn(async move {
            // We need to get the handle from the HashMap and await it
            let join_result = {
                let mut handles = handles_cleanup.write().await;
                if let Some(handle) = handles.remove(&cleanup_task_id) {
                    Some(handle.await)
                } else {
                    None
                }
            };

            if let Some(_) = join_result {
                let mut tasks = tasks_cleanup.write().await;
                if let Some(_) = tasks.remove(&cleanup_task_id) {
                    debug!("Cleaned up task '{}' ({})", cleanup_name, cleanup_task_id);
                }
            }
        });

        Ok(TaskHandle {
            id: task_id,
            name,
            _manager: Arc::clone(&self.tasks),
        })
    }

    async fn watchdog_monitor(
        name: String,
        task_id: Uuid,
        mut watchdog_rx: broadcast::Receiver<()>,
        timeout: Duration,
        enable_logging: bool,
    ) {
        let mut interval = interval(timeout);
        let mut last_reset = Instant::now();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let elapsed = last_reset.elapsed();
                    if elapsed >= timeout {
                        warn!(
                            "Watchdog timeout for task '{}' ({}): no heartbeat for {:?}",
                            name, task_id, elapsed
                        );
                    }
                }
                result = watchdog_rx.recv() => {
                    match result {
                        Ok(()) => {
                            let elapsed = last_reset.elapsed();
                            if enable_logging {
                                debug!(
                                    "Watchdog reset for task '{}' ({}): {:?} since last reset",
                                    name, task_id, elapsed
                                );
                            }
                            last_reset = Instant::now();
                        }
                        Err(_) => {
                            // Watchdog channel closed, task probably finished
                            debug!("Watchdog for task '{}' ({}) shutting down", name, task_id);
                            break;
                        }
                    }
                }
            }
        }
    }

    pub async fn wait_for_task(
        &self,
        handle: &TaskHandle,
        timeout_duration: Option<Duration>,
    ) -> Result<(), TaskError> {
        let join_handle = {
            let mut task_handles = self.task_handles.write().await;
            task_handles.remove(&handle.id)
        };

        if let Some(join_handle) = join_handle {
            match timeout_duration {
                Some(dur) => match timeout(dur, join_handle).await {
                    Ok(result) => result.map_err(|e| TaskError::JoinError(e))?,
                    Err(_) => return Err(TaskError::Timeout),
                },
                None => join_handle.await.map_err(|e| TaskError::JoinError(e))?,
            }
        } else {
            return Err(TaskError::NotFound);
        }

        Ok(())
    }

    pub async fn cancel_task(
        &self,
        handle: &TaskHandle,
        timeout_duration: Option<Duration>,
    ) -> Result<(), TaskError> {
        let join_handle = {
            let mut task_handles = self.task_handles.write().await;
            task_handles.remove(&handle.id)
        };

        if let Some(join_handle) = join_handle {
            join_handle.abort();

            match timeout_duration {
                Some(dur) => match timeout(dur, join_handle).await {
                    Ok(_) => {}
                    Err(_) => warn!("Task '{}' did not cancel within timeout", handle.name),
                },
                None => {
                    let _ = join_handle.await;
                }
            }
        }

        Ok(())
    }

    pub async fn current_tasks(&self) -> Vec<TaskInfo> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .map(|task| TaskInfo {
                id: task.id,
                name: task.name.clone(),
                created_at: task.created_at,
                watchdog_enabled: task.enable_watchdog_timers,
            })
            .collect()
    }

    pub async fn shutdown(&self) -> Result<(), TaskError> {
        info!("Shutting down task manager");
        let _ = self.shutdown_tx.send(());

        // Wait for all tasks to complete with timeout
        let task_handles: Vec<_> = {
            let mut task_handles = self.task_handles.write().await;
            task_handles.drain().map(|(_, handle)| handle).collect()
        };

        for handle in task_handles {
            let _ = timeout(Duration::from_secs(10), handle).await;
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    pub enable_timers: bool,
    pub enable_logging: bool,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct TaskContext {
    pub id: Uuid,
    pub name: String,
    watchdog_tx: Option<broadcast::Sender<()>>,
}

impl TaskContext {
    pub fn reset_watchdog(&self) {
        if let Some(tx) = &self.watchdog_tx {
            if let Err(_) = tx.send(()) {
                // Channel closed, task probably finishing
            }
        }
    }

    pub fn watchdog_enabled(&self) -> bool {
        self.watchdog_tx.is_some()
    }
}

#[derive(Debug)]
pub struct TaskHandle {
    pub id: Uuid,
    pub name: String,
    _manager: Arc<RwLock<HashMap<Uuid, TaskData>>>,
}

#[derive(Debug, Clone)]
pub struct TaskInfo {
    pub id: Uuid,
    pub name: String,
    pub created_at: Instant,
    pub watchdog_enabled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("Task timed out")]
    Timeout,
    #[error("Join error: {0}")]
    JoinError(tokio::task::JoinError),
    #[error("Task not found")]
    NotFound,
}

// Example usage and tests
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_basic_task_creation() {
        let config = TaskManagerConfig::default();
        let manager = TaskManager::new(config);

        let handle = manager
            .create_task(
                "test_task".to_string(),
                |ctx| async move {
                    println!("Task {} running", ctx.name);
                    sleep(Duration::from_millis(100)).await;
                },
                None,
            )
            .await
            .unwrap();

        manager.wait_for_task(&handle, None).await.unwrap();
    }

    #[tokio::test]
    async fn test_watchdog_functionality() {
        let mut config = TaskManagerConfig::default();
        config.enable_watchdog_timers = true;
        config.enable_watchdog_logging = true;
        config.default_watchdog_timeout = Duration::from_millis(100);

        let manager = TaskManager::new(config);

        let handle = manager
            .create_task(
                "watchdog_test".to_string(),
                |ctx| async move {
                    for i in 0..5 {
                        println!("Working on iteration {}", i);
                        sleep(Duration::from_millis(50)).await;
                        ctx.reset_watchdog(); // Reset before timeout
                    }
                },
                None,
            )
            .await
            .unwrap();

        manager.wait_for_task(&handle, None).await.unwrap();
    }

    #[tokio::test]
    async fn test_task_cancellation() {
        let config = TaskManagerConfig::default();
        let manager = TaskManager::new(config);

        let handle = manager
            .create_task(
                "long_task".to_string(),
                |_ctx| async move {
                    sleep(Duration::from_secs(10)).await;
                },
                None,
            )
            .await
            .unwrap();

        // Cancel after a short delay
        tokio::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            manager
                .cancel_task(&handle, Some(Duration::from_secs(1)))
                .await
                .unwrap();
        });
    }
}
