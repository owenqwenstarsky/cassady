use super::{schema, ToolContext, ToolRuntimeEvent, ToolSpec};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Debug, Deserialize)]
struct Args {
    command: String,
    #[serde(default)]
    timeout: Option<u64>,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "shell".into(),
        description: "Run a shell command in the launch cwd. Requires full-access mode. Streams stdout/stderr while running, then returns stdout, stderr, and exit code. Use timeout (seconds) to limit runtime."
            .into(),
        parameters: schema::object(
            json!({
                "command": {"type": "string", "description": "Shell command to execute"},
                "timeout": {"type": "integer", "description": "Optional timeout in seconds (default 30)"}
            }),
            &["command"],
        ),
    }
}

pub async fn run(args: Value, ctx: &ToolContext) -> Result<String> {
    let args: Args = serde_json::from_value(args)?;
    if !ctx.mode.can_write() {
        bail!("shell tool requires full-access mode");
    }

    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(&args.command);
    cmd.current_dir(&ctx.cwd);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().context("spawning shell command")?;
    let stdout = child.stdout.take().context("capturing command stdout")?;
    let stderr = child.stderr.take().context("capturing command stderr")?;

    let stdout_tx = ctx.runtime_tx.clone();
    let stderr_tx = ctx.runtime_tx.clone();
    let stdout_task = tokio::spawn(read_stream("stdout", stdout, stdout_tx));
    let stderr_task = tokio::spawn(read_stream("stderr", stderr, stderr_tx));

    let timeout = Duration::from_secs(args.timeout.unwrap_or(30));
    let mut timed_out = false;
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(status) => status.context("waiting for shell command")?,
        Err(_) => {
            timed_out = true;
            let _ = child.kill().await;
            child
                .wait()
                .await
                .context("waiting for timed-out shell command to exit")?
        }
    };

    let stdout = stdout_task
        .await
        .context("joining stdout reader")?
        .context("reading command stdout")?;
    let stderr = stderr_task
        .await
        .context("joining stderr reader")?
        .context("reading command stderr")?;

    let result = format_result(&stdout, &stderr, status.code().unwrap_or(-1));
    if timed_out {
        bail!("command timed out after {}s\n{}", timeout.as_secs(), result);
    }
    Ok(result)
}

async fn read_stream<R>(
    stream: &'static str,
    mut reader: R,
    tx: Option<tokio::sync::mpsc::UnboundedSender<ToolRuntimeEvent>>,
) -> Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut collected = Vec::new();
    let mut buf = [0_u8; 4096];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        collected.extend_from_slice(chunk);
        if let Some(tx) = &tx {
            let _ = tx.send(ToolRuntimeEvent::OutputChunk {
                stream: stream.to_string(),
                content: String::from_utf8_lossy(chunk).to_string(),
            });
        }
    }
    Ok(collected)
}

fn format_result(stdout: &[u8], stderr: &[u8], code: i32) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);

    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str("stdout:\n");
        result.push_str(&stdout);
        if !stdout.ends_with('\n') {
            result.push('\n');
        }
    }
    if !stderr.is_empty() {
        result.push_str("stderr:\n");
        result.push_str(&stderr);
        if !stderr.ends_with('\n') {
            result.push('\n');
        }
    }
    if result.is_empty() {
        result.push_str("(no output)\n");
    }
    result.push_str(&format!("exit code: {}\n", code));
    result
}
