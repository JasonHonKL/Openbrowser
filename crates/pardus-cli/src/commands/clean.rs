use std::path::PathBuf;

use anyhow::Result;

pub fn run(cache_dir: Option<PathBuf>, cookies_only: bool, cache_only: bool) -> Result<()> {
    let dir = cache_dir.unwrap_or_else(|| {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("pardus-browser")
    });

    if !dir.exists() {
        println!("Cache directory does not exist: {}", dir.display());
        return Ok(());
    }

    if cookies_only {
        let cookies_file = dir.join("cookies.json");
        if cookies_file.exists() {
            std::fs::remove_file(&cookies_file)?;
            println!("Removed cookies: {}", cookies_file.display());
        }
    } else if cache_only {
        let cache_sub = dir.join("cache");
        if cache_sub.exists() {
            std::fs::remove_dir_all(&cache_sub)?;
            println!("Removed cache: {}", cache_sub.display());
        }
    } else {
        std::fs::remove_dir_all(&dir)?;
        println!("Removed entire cache directory: {}", dir.display());
    }

    Ok(())
}
