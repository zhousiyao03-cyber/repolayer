use crate::config::Config;
use crate::indexer::Indexer;
use anyhow::Result;
use tracing::info;

pub async fn run() -> Result<()> {
    let workspace = std::env::current_dir()?;
    let cfg_path = workspace.join("repolayer.yml");
    let cfg = Config::from_path(&cfg_path)?;
    let db_path = workspace.join(".repolayer").join("index.db");
    let mut indexer = Indexer::new(workspace, db_path.clone(), cfg)?;
    let stats = indexer.build_all().await?;
    info!(
        "indexed {} nodes, {} edges → {}",
        stats.nodes,
        stats.edges,
        db_path.display()
    );
    println!("indexed {} nodes, {} edges", stats.nodes, stats.edges);
    eprintln!(
        "repolayer: build complete — nodes={}, edges={}, embed_requests={}, embed_retries={}, embed_chars={}, summary_count={}",
        stats.nodes,
        stats.edges,
        stats.embed_requests,
        stats.embed_retries,
        stats.embed_input_chars,
        stats.summary_count
    );
    Ok(())
}
