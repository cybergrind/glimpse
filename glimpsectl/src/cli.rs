use glimpse_client::{Client, SubscriptionEvent};

use crate::format::format_json;

pub async fn cmd_get(topic: String, color: bool, pretty: bool) -> anyhow::Result<()> {
    let client = Client::connect().await?;
    let data = client.get(&topic).await;

    match data {
        Ok(value) => {
            println!("{}", format_json(&value, color, pretty));
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }

    Ok(())
}

pub async fn cmd_subscribe(patterns: Vec<String>, color: bool, pretty: bool) -> anyhow::Result<()> {
    let client = Client::connect().await?;
    let mut subs = Vec::new();

    for pattern in &patterns {
        match client.subscribe(pattern).await {
            Ok(sub) => {
                eprintln!("subscribed to {pattern}");
                subs.push(sub);
            }
            Err(e) => {
                eprintln!("subscribe failed for {pattern}: {e}");
            }
        }
    }

    if subs.is_empty() {
        anyhow::bail!("no active subscriptions");
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<SubscriptionEvent>(64);
    for mut sub in subs {
        let tx = tx.clone();
        tokio::spawn(async move {
            while let Some(event) = sub.next().await {
                if tx.send(event).await.is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);

    while let Some(event) = rx.recv().await {
        let value = serde_json::json!({"topic": event.topic, "ts": event.ts, "data": event.data});
        println!("{}", format_json(&value, color, pretty));
    }

    eprintln!("daemon disconnected");
    std::process::exit(1);
}

pub async fn cmd_inspect(
    filter: Vec<String>,
    topics_only: bool,
    methods_only: bool,
) -> anyhow::Result<()> {
    let client = Client::connect().await?;
    let data = client.get("inspect.providers").await?;

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
    let client = Client::connect().await?;
    let data = client.call(&method, params).await;

    match data {
        Ok(value) => {
            println!("{}", format_json(&value, color, pretty));
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }

    Ok(())
}
