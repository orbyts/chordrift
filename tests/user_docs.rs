use clap::CommandFactory;

use chordrift::cli::Cli;

#[test]
fn user_guide_mentions_every_cli_leaf_command() {
    let guide = include_str!("../docs/HOW_TO_CHORDRIFT.md");
    let command = Cli::command();
    let mut leaf_commands = Vec::new();
    collect_leaf_commands(&command, "chordrift", &mut leaf_commands);

    for command in leaf_commands {
        assert!(
            guide.contains(&command),
            "docs/HOW_TO_CHORDRIFT.md must document `{command}`"
        );
    }
}

fn collect_leaf_commands(command: &clap::Command, prefix: &str, output: &mut Vec<String>) {
    let children: Vec<_> = command
        .get_subcommands()
        .filter(|child| !matches!(child.get_name(), "help"))
        .collect();
    if children.is_empty() {
        output.push(prefix.to_owned());
        return;
    }

    for child in children {
        collect_leaf_commands(child, &format!("{prefix} {}", child.get_name()), output);
    }
}
