use anyhow::Context;
use glimpse_types::{Request, Response};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub struct Connection {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl Connection {
    pub async fn connect() -> anyhow::Result<Self> {
        let path = glimpse_types::socket_path()?;
        let stream = UnixStream::connect(&path)
            .await
            .with_context(|| format!("failed to connect to {}", path.display()))?;
        let (read, write) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(read),
            writer: write,
        })
    }

    pub async fn send(&mut self, request: &Request) -> anyhow::Result<()> {
        let mut line = serde_json::to_string(request)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        Ok(())
    }

    pub async fn recv(&mut self) -> anyhow::Result<Option<Response>> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None);
        }
        let response = serde_json::from_str(&line)?;
        Ok(Some(response))
    }
}
