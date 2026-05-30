use crate::config::Config;
use crate::indexer::incremental::update;
use anyhow::Result;

pub async fn run() -> Result<()> {
    let workspace = std::env::current_dir()?;
    let cfg = Config::from_path(&workspace.join("repolayer.yml"))?;
    let db = workspace.join(".repolayer/index.db");
    if !db.exists() {
        anyhow::bail!(
            "no index found at {} — run `repolayer build` first",
            db.display()
        );
    }
    update(workspace, db, cfg).await
}
