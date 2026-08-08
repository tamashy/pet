use std::path::Path;

use anyhow::Result;

use crate::config::Config;
use crate::editor;

pub fn run(config: &Config, config_path: &Path) -> Result<()> {
    editor::open(&config.general, config_path, 0)
}
