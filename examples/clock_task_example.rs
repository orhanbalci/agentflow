//! Example demonstrating clock task timed frame delivery

use agentflow::frames::{FrameType, StartFrame, EndFrame};
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Clock Task Timed Frame Delivery Example");
    println!("=======================================");

    // Demonstrate the clock task concept
    println!("The clock task handler implements Python's _clock_task_handler:");
    println!("1. Receives timed frames from a priority queue");
    println!("2. Waits until the frame's presentation timestamp");
    println!("3. Pushes frames downstream to the transport");
    println!();

    // Show how timestamps work
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    
    let future_time = current_time + 1_000_000_000; // 1 second in the future
    
    println!("Current time (nanoseconds): {}", current_time);
    println!("Future time (nanoseconds):  {}", future_time);
    println!("Wait time: {} seconds", (future_time - current_time) as f64 / 1_000_000_000.0);
    println!();

    // Demonstrate frame types that could be scheduled
    let start_frame = FrameType::Start(StartFrame::new());
    let end_frame = FrameType::End(EndFrame::new());
    
    println!("Frame types that can be scheduled:");
    println!("- StartFrame: {:?}", std::mem::discriminant(&start_frame));
    println!("- EndFrame: {:?}", std::mem::discriminant(&end_frame));
    println!();

    println!("Python equivalent code:");
    println!("```python");
    println!("async def _clock_task_handler(self):");
    println!("    running = True");
    println!("    while running:");
    println!("        timestamp, _, frame = await self._clock_queue.get()");
    println!("        running = not isinstance(frame, EndFrame)");
    println!("        if running:");
    println!("            current_time = self._transport.get_clock().get_time()");
    println!("            if timestamp > current_time:");
    println!("                wait_time = nanoseconds_to_seconds(timestamp - current_time)");
    println!("                await asyncio.sleep(wait_time)");
    println!("            await self._transport.push_frame(frame)");
    println!("        self._clock_queue.task_done()");
    println!("```");
    println!();

    println!("Rust equivalent (implemented):");
    println!("- Uses mpsc::UnboundedReceiver for the clock queue");
    println!("- Waits using tokio::time::sleep");
    println!("- Handles EndFrame to stop the task");
    println!("- TODO: Integrate with transport's get_clock() and push_frame()");

    Ok(())
}
