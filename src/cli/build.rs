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
    Ok(())
}
