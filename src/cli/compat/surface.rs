//! `repolayer surface [path]` — resolve the published public API of a package.
//!
//! Auto-detects the package type from manifest files (Cargo.toml, pyproject.toml,
//! package.json, __init__.py) and follows re-exports to show exactly the symbols
//! a downstream consumer can reach.

use anyhow::Result;
use std::path::PathBuf;

pub async fn run(path: PathBuf, json: bool) -> Result<()> {
    use crate::surface::options::{OutputMode, SurfaceOptions};
    use crate::surface::render;

    let opts = if json {
        SurfaceOptions {
            output: OutputMode::Json { compact: false },
            ..SurfaceOptions::default()
        }
    } else {
        SurfaceOptions::default()
    };

    let entries = crate::surface::resolve_surface(&path, &opts)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let text = render::render(&entries, opts.output, opts.include_chain);
    print!("{}", text);

    Ok(())
}
