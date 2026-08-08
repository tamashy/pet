use pet::config::GeneralConfig;
use pet::snippet::Snippets;

fn general_for(snippetfile: &std::path::Path, snippetdirs: Vec<String>) -> GeneralConfig {
    GeneralConfig {
        snippetfile: snippetfile.to_string_lossy().into_owned(),
        snippetdirs,
        ..GeneralConfig::default()
    }
}

#[test]
fn loads_multiline_command_tags_and_missing_output() {
    let dir = tempfile::tempdir().unwrap();
    let snippet_file = dir.path().join("snippet.toml");
    std::fs::write(
        &snippet_file,
        r#"
[[snippets]]
  description = "multi"
  command = """
echo one
echo two"""
  tag = ["a", "b"]
"#,
    )
    .unwrap();

    let general = general_for(&snippet_file, vec![]);
    let snippets = Snippets::load(&general, false).unwrap();

    assert_eq!(snippets.snippets.len(), 1);
    let s = &snippets.snippets[0];
    assert_eq!(s.description, "multi");
    assert_eq!(s.command, "echo one\necho two");
    assert_eq!(s.tag, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(s.output, "");
    assert_eq!(s.filename, snippet_file);
}

#[test]
fn missing_snippet_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    let general = general_for(&dir.path().join("does-not-exist.toml"), vec![]);
    let err = Snippets::load(&general, false).unwrap_err();
    assert!(matches!(
        err,
        pet::error::SnippetError::SnippetFileNotFound(_)
    ));
}

#[test]
fn missing_snippet_dir_errors() {
    let dir = tempfile::tempdir().unwrap();
    let snippet_file = dir.path().join("snippet.toml");
    std::fs::write(&snippet_file, "").unwrap();

    let general = general_for(
        &snippet_file,
        vec![dir.path().join("nope").to_string_lossy().into_owned()],
    );
    let err = Snippets::load(&general, true).unwrap_err();
    assert!(matches!(
        err,
        pet::error::SnippetError::SnippetDirNotFound(_)
    ));
}

#[test]
fn merges_snippetfile_and_snippetdirs_and_saves_back_to_origin_only() {
    let dir = tempfile::tempdir().unwrap();
    let snippet_file = dir.path().join("snippet.toml");
    std::fs::write(
        &snippet_file,
        "[[snippets]]\n  description = \"main\"\n  command = \"echo main\"\n",
    )
    .unwrap();

    let extra_dir = dir.path().join("extra");
    std::fs::create_dir(&extra_dir).unwrap();
    let extra_file = extra_dir.join("more.toml");
    std::fs::write(
        &extra_file,
        "[[snippets]]\n  description = \"extra\"\n  command = \"echo extra\"\n",
    )
    .unwrap();

    let general = general_for(
        &snippet_file,
        vec![extra_dir.to_string_lossy().into_owned()],
    );
    let mut snippets = Snippets::load(&general, true).unwrap();
    assert_eq!(snippets.snippets.len(), 2);

    // Mutate both snippets and save; each should be rewritten to its own origin file,
    // leaving the other file untouched by the edit.
    for s in &mut snippets.snippets {
        s.output = "touched".to_string();
    }
    snippets.save(&general).unwrap();

    let reloaded = Snippets::load(&general, true).unwrap();
    assert_eq!(reloaded.snippets.len(), 2);
    assert!(reloaded.snippets.iter().all(|s| s.output == "touched"));

    let main_contents = std::fs::read_to_string(&snippet_file).unwrap();
    assert!(main_contents.contains("main"));
    assert!(!main_contents.contains("extra"));

    let extra_contents = std::fs::read_to_string(&extra_file).unwrap();
    assert!(extra_contents.contains("extra"));
    assert!(!extra_contents.contains("\"main\""));
}

#[test]
fn order_sorts_by_each_sortby_value() {
    let mut snippets = Snippets {
        snippets: vec![
            make("b desc", "b cmd", "b out"),
            make("a desc", "a cmd", "a out"),
            make("c desc", "c cmd", "c out"),
        ],
    };

    snippets.order("description");
    assert_eq!(descs(&snippets), vec!["c desc", "b desc", "a desc"]);

    snippets.order("-description");
    assert_eq!(descs(&snippets), vec!["a desc", "b desc", "c desc"]);

    let mut by_command = snippets.clone();
    by_command.order("command");
    assert_eq!(cmds(&by_command), vec!["c cmd", "b cmd", "a cmd"]);

    let mut by_output = snippets.clone();
    by_output.order("output");
    assert_eq!(outs(&by_output), vec!["c out", "b out", "a out"]);

    let original_order = descs(&snippets);
    let mut reversed = snippets.clone();
    reversed.order("-recency");
    assert_eq!(
        descs(&reversed),
        original_order.into_iter().rev().collect::<Vec<_>>()
    );

    let mut untouched = snippets.clone();
    untouched.order("");
    assert_eq!(descs(&untouched), descs(&snippets));

    fn make(desc: &str, cmd: &str, out: &str) -> pet::snippet::SnippetInfo {
        pet::snippet::SnippetInfo {
            filename: Default::default(),
            description: desc.to_string(),
            command: cmd.to_string(),
            tag: vec![],
            output: out.to_string(),
        }
    }
    fn descs(s: &Snippets) -> Vec<String> {
        s.snippets.iter().map(|x| x.description.clone()).collect()
    }
    fn cmds(s: &Snippets) -> Vec<String> {
        s.snippets.iter().map(|x| x.command.clone()).collect()
    }
    fn outs(s: &Snippets) -> Vec<String> {
        s.snippets.iter().map(|x| x.output.clone()).collect()
    }
}

#[test]
fn filter_by_tags_requires_at_least_one_matching_tag() {
    let snippets = Snippets {
        snippets: vec![
            tagged("has-a", vec!["a"]),
            tagged("has-b-c", vec!["b", "c"]),
            tagged("no-tags", vec![]),
            tagged("has-z", vec!["z"]),
        ],
    };

    let filtered = snippets.filter_by_tags(&["a".to_string(), "c".to_string()]);
    let descs: Vec<&str> = filtered.iter().map(|s| s.description.as_str()).collect();
    assert_eq!(descs, vec!["has-a", "has-b-c"]);

    fn tagged(desc: &str, tags: Vec<&str>) -> pet::snippet::SnippetInfo {
        pet::snippet::SnippetInfo {
            filename: Default::default(),
            description: desc.to_string(),
            command: "echo".to_string(),
            tag: tags.into_iter().map(String::from).collect(),
            output: String::new(),
        }
    }
}
