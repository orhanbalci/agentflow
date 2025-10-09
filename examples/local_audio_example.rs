//! Example demonstrating local audio transport usage.
//!
//! This example shows how to create and use a local audio transport
//! for real-time audio input and output.

use agentflow::task_manager::{TaskManager, TaskManagerConfig};
use agentflow::transport::local::{LocalAudioTransport, LocalAudioTransportParams};
use agentflow::transport::params::TransportParams;
use agentflow::StartFrame;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("Local Audio Transport Example");
    println!("=============================");

    // List available audio devices
    println!("\nAvailable Input Devices:");
    match LocalAudioTransport::list_input_devices() {
        Ok(devices) => {
            for (i, device) in devices.iter().enumerate() {
                println!("  {}: {}", i, device);
            }
        }
        Err(e) => println!("  Error listing input devices: {}", e),
    }

    println!("\nAvailable Output Devices:");
    match LocalAudioTransport::list_output_devices() {
        Ok(devices) => {
            for (i, device) in devices.iter().enumerate() {
                println!("  {}: {}", i, device);
            }
        }
        Err(e) => println!("  Error listing output devices: {}", e),
    }

    // Get default devices
    println!("\nDefault Devices:");
    match LocalAudioTransport::default_input_device_name() {
        Ok(name) => println!("  Input: {}", name),
        Err(e) => println!("  Error getting default input: {}", e),
    }

    match LocalAudioTransport::default_output_device_name() {
        Ok(name) => println!("  Output: {}", name),
        Err(e) => println!("  Error getting default output: {}", e),
    }

    // Create task manager
    let config = TaskManagerConfig::default();
    let task_manager = Arc::new(TaskManager::new(config));

    // Create transport parameters
    let mut base_params = TransportParams::default();
    base_params.audio_in_enabled = true;
    base_params.audio_out_enabled = true;
    base_params.audio_in_sample_rate = Some(44100);
    base_params.audio_out_sample_rate = Some(44100);
    base_params.audio_in_channels = 1;
    base_params.audio_out_channels = 2;

    let local_params = LocalAudioTransportParams::new(base_params).with_buffer_size(1024);

    // Create the transport
    let transport = LocalAudioTransport::new(local_params, task_manager);

    // Create start frame
    let start_frame = StartFrame::new().with_sample_rates(16000, 16000);

    println!("\nStarting audio transport...");
    transport.start(&start_frame).await?;

    println!("Audio transport started successfully!");
    println!("Input running: {}", transport.is_input_running().await);
    println!("Output running: {}", transport.is_output_running().await);
    println!("Input sample rate: {}", transport.input_sample_rate().await);

    // Run for a few seconds
    println!("\nRunning for 5 seconds...");
    sleep(Duration::from_secs(5)).await;

    // Stop the transport
    println!("\nStopping audio transport...");
    transport.stop().await?;

    println!("Audio transport stopped successfully!");

    Ok(())
}
