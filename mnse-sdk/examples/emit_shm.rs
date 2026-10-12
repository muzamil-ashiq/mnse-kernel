use mnse_sdk::SlotBusClient;
use mnse_types::{Event, EventType};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let client = SlotBusClient::default_local();
    let event = Event::new(
        EventType::ProcessExecuted {
            command: "echo 'SlotBus zero-copy test'".into(),
            exit_code: 0,
            duration_ms: 1,
            working_dir: "/home/mab/Applications/my-project".into(),
            stdout_tail: Some("SlotBus zero-copy test".into()),
            stderr_tail: None,
        },
        "shell",
    );

    let start = Instant::now();
    client.emit(&event)?;
    let elapsed = start.elapsed();

    println!("SUCCESS: Emitted event to MNSE Kernel via SlotBus shared memory in {:?}", elapsed);
    Ok(())
}
