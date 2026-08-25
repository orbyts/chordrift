use clap::CommandFactory;

use chordrift::cli::Cli;

#[test]
fn user_guide_mentions_every_cli_leaf_command() {
    let guide = concat!(
        include_str!("../docs/HOW_TO_CHORDRIFT.md"),
        include_str!("../docs/reference/CLI_COMMANDS.md"),
        include_str!("../docs/how-to/ADDING_AND_DISCOVERY.md"),
        include_str!("../docs/how-to/DELETING_AND_EXCLUDING.md"),
        include_str!("../docs/how-to/ROUTING_AND_RECLASSIFYING.md"),
        include_str!("../docs/how-to/CLASSIFICATION_DIMENSIONS.md"),
        include_str!("../docs/how-to/SYNC_AND_CONVERGENCE.md"),
    );
    let command = Cli::command();
    let mut leaf_commands = Vec::new();
    collect_leaf_commands(&command, "chordrift", &mut leaf_commands);

    for command in leaf_commands {
        assert!(
            guide.contains(&command),
            "the user guide or CLI reference must document `{command}`"
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
