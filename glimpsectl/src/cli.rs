use glimpse_types::{Request, RequestResult, Response};

use crate::connection::Connection;
use crate::format::format_json;

pub async fn cmd_get(topic: String, color: bool, pretty: bool) -> anyhow::Result<()> {
    let mut conn = Connection::connect().await?;
    conn.send(&Request::Get { topic }).await?;

    let Some(resp) = conn.recv().await? else {
        anyhow::bail!("connection closed");
    };

    match resp {
        Response::GetResult { result, .. } => match result {
            RequestResult::Ok { data } => {
                println!("{}", format_json(&data, color, pretty));
            }
            RequestResult::Error { code, message } => {
                eprintln!("error {code}: {message}");
                std::process::exit(1);
            }
        },
        other => {
            let value = serde_json::to_value(&other)?;
            println!("{}", format_json(&value, color, pretty));
        }
    }

    Ok(())
}

pub async fn cmd_subscribe(
    patterns: Vec<String>,
    color: bool,
    pretty: bool,
) -> anyhow::Result<()> {
    let mut conn = Connection::connect().await?;

    for pattern in &patterns {
        conn.send(&Request::Subscribe {
            pattern: pattern.clone(),
        })
        .await?;
    }

    loop {
        let Some(resp) = conn.recv().await? else {
            eprintln!("daemon disconnected");
            std::process::exit(1);
        };

        match &resp {
            Response::Event { topic, data } => {
                if pretty || color {
                    eprintln!("\x1b[2m{topic}\x1b[0m");
                    println!("{}", format_json(data, color, pretty));
                } else {
                    let value = serde_json::to_value(&resp)?;
                    println!("{}", format_json(&value, false, false));
                }
            }
            Response::SubscribeAck {
                pattern,
                available,
                error,
            } => {
                if *available {
                    eprintln!("subscribed to {pattern}");
                } else {
                    eprintln!(
                        "subscribe failed for {pattern}: {}",
                        error.as_deref().unwrap_or("unknown")
                    );
                }
            }
            Response::ProviderUnavailable { provider, error } => {
                eprintln!("provider {provider} unavailable: {error}");
            }
            other => {
                let value = serde_json::to_value(other)?;
                println!("{}", format_json(&value, color, pretty));
            }
        }
    }
}

pub async fn cmd_inspect(
    filter: Vec<String>,
    topics_only: bool,
    methods_only: bool,
) -> anyhow::Result<()> {
    let mut conn = Connection::connect().await?;
    conn.send(&Request::Get {
        topic: "inspect.providers".into(),
    })
    .await?;

    let Some(resp) = conn.recv().await? else {
        anyhow::bail!("connection closed");
    };

    let data = match resp {
        Response::GetResult { result, .. } => match result {
            RequestResult::Ok { data } => data,
            RequestResult::Error { code, message } => {
                eprintln!("error {code}: {message}");
                std::process::exit(1);
            }
        },
        _ => anyhow::bail!("unexpected response"),
    };

    let Some(providers) = data.as_array() else {
        anyhow::bail!("unexpected response format");
    };

    for provider in providers {
        let name = provider["name"].as_str().unwrap_or("?");

        if !filter.is_empty() && !filter.iter().any(|f| f == name) {
            continue;
        }

        let topics: Vec<&str> = provider["topics"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        let methods: Vec<&str> = provider["methods"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        if topics_only {
            for t in &topics {
                println!("{t}");
            }
        } else if methods_only {
            for m in &methods {
                println!("{m}");
            }
        } else {
            println!("{name}:");
            if !topics.is_empty() {
                println!("  topics:");
                for t in &topics {
                    println!("    - {t}");
                }
            }
            if !methods.is_empty() {
                println!("  methods:");
                for m in &methods {
                    println!("    - {m}");
                }
            }
        }
    }

    Ok(())
}

pub async fn cmd_call(
    method: String,
    params: serde_json::Value,
    color: bool,
    pretty: bool,
) -> anyhow::Result<()> {
    let mut conn = Connection::connect().await?;
    conn.send(&Request::Call { method, params }).await?;

    let Some(resp) = conn.recv().await? else {
        anyhow::bail!("connection closed");
    };

    match resp {
        Response::CallResult { result, .. } => match result {
            RequestResult::Ok { data } => {
                println!("{}", format_json(&data, color, pretty));
            }
            RequestResult::Error { code, message } => {
                eprintln!("error {code}: {message}");
                std::process::exit(1);
            }
        },
        other => {
            let value = serde_json::to_value(&other)?;
            println!("{}", format_json(&value, color, pretty));
        }
    }

    Ok(())
}
