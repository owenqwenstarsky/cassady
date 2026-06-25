use cassady::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let session = SessionBuilder::new()
        .cwd(std::env::current_dir()?)
        .access_mode(AccessMode::ReadOnly)
        .build()
        .await?;

    let mut turn = session
        .start_turn("Summarize this project in a few sentences.")
        .await?;

    while let Some(event) = turn.next_event().await? {
        match event {
            Event::AssistantChunk(text) => print!("{text}"),
            Event::ApprovalRequested(request) => {
                eprintln!(
                    "approval requested for {}: {}; denying in this example",
                    request.name, request.reason
                );
                turn.deny(&request.request_id)?;
            }
            Event::Finished => break,
            _ => {}
        }
    }

    let session = turn.finish().await?;
    eprintln!("\nResume chat with: cass --resume {}", session.id());
    Ok(())
}
