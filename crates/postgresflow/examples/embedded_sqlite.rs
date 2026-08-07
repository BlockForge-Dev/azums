use postgresflow::{quickstart, Job};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_path = "sqlite://embedded_jobs.db?mode=rwc";
    println!("Connecting to embedded SQLite backend at: {db_path}");

    let flow = quickstart(db_path).await?;

    flow.enqueue(Job::new(
        "sensor_telemetry",
        serde_json::json!({
            "device_id": "rpi-edge-001",
            "temperature_celsius": 23.5,
            "humidity_percent": 48.2
        }),
    ))
    .await?;

    flow.register_handler("sensor_telemetry", |job| async move {
        println!(
            "Processed edge telemetry from {}: temp={}°C, humidity={}%",
            job.payload["device_id"],
            job.payload["temperature_celsius"],
            job.payload["humidity_percent"]
        );
        Ok(())
    })
    .await;

    let processed = flow.run_until_empty().await?;
    println!("Successfully processed {processed} jobs on zero-network SQLite storage!");

    // Clean up temporary sqlite files
    let _ = std::fs::remove_file("embedded_jobs.db");
    let _ = std::fs::remove_file("embedded_jobs.db-wal");
    let _ = std::fs::remove_file("embedded_jobs.db-shm");

    Ok(())
}
