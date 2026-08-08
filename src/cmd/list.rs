use anyhow::Result;
use owo_colors::OwoColorize;

use crate::config::Config;
use crate::format::truncate_pad;
use crate::snippet::Snippets;

pub fn run(
    config: &Config,
    oneline: bool,
    tags: Option<&str>,
    debug: bool,
) -> Result<()> {
    let mut snippets = Snippets::load(&config.general, true)?;

    if let Some(tags) = tags {
        let tag_list: Vec<String> = tags.split(',').map(|s| s.trim().to_string()).collect();
        snippets.snippets = snippets.filter_by_tags(&tag_list);
    }

    let col = if config.general.column <= 0 {
        40
    } else {
        config.general.column as usize
    };

    for s in &snippets.snippets {
        if oneline {
            let description = truncate_pad(&s.description, col);
            let command = s.command.replace('\n', "\\n");
            println!("{} : {}", description.bright_green(), command.bright_yellow());
            continue;
        }

        if debug {
            let label = format!("{:>12}", "Filename:");
            println!("{} {}", label.red(), s.filename.display());
        }

        let label = format!("{:>12}", "Description:");
        println!("{} {}", label.bright_green(), s.description);

        let mut lines = s.command.split('\n');
        let label = format!("{:>12}", "Command:");
        println!("{} {}", label.bright_yellow(), lines.next().unwrap_or(""));
        for line in lines {
            println!("{:>12} {}", "", line);
        }

        if !s.tag.is_empty() {
            let label = format!("{:>12}", "Tag:");
            println!("{} {}", label.bright_cyan(), s.tag.join(" "));
        }

        if !s.output.is_empty() {
            let output = s.output.replace('\n', "\n             ");
            let label = format!("{:>12}", "Output:");
            println!("{} {}", label.bright_red(), output);
        }

        println!("{}", "-".repeat(30));
    }

    Ok(())
}
