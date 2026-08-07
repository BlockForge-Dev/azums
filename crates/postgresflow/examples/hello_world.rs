use postgresflow::{quickstart, Job};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let flow = quickstart("postgres://localhost/flow").await?;
    flow.enqueue(Job::new("greet", serde_json::json!({"name": "World"})))
        .await?;
    flow.register_handler("greet", |job| async move {
        println!("Hello, {}!", job.payload["name"]);
        Ok(())
    })
    .await;
    flow.run().await?;
    Ok(())
}
