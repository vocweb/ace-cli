use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use crate::args::{format_connected_line, resolve_repl_model, AllowedToolSet, CliOutputFormat};
use crate::cli::LiveCli;
use commands::{resolve_skill_invocation, SkillSlashDispatch, SlashCommand};
use runtime::{
    check_base_commit, format_stale_base_warning, resolve_expected_base, PermissionMode,
};

/// Detect if the current working directory is "broad" (home directory or
/// filesystem root). Returns the cwd path if broad, None otherwise.
fn detect_broad_cwd() -> Option<PathBuf> {
    let Ok(cwd) = std::env::current_dir() else {
        return None;
    };
    let is_home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h) == cwd)
        .unwrap_or(false);
    let is_root = cwd.parent().is_none();
    if is_home || is_root {
        Some(cwd)
    } else {
        None
    }
}

pub(crate) fn enforce_broad_cwd_policy(
    allow_broad_cwd: bool,
    output_format: CliOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    if allow_broad_cwd {
        return Ok(());
    }
    let Some(cwd) = detect_broad_cwd() else {
        return Ok(());
    };

    let is_interactive = io::stdin().is_terminal();

    if is_interactive {
        // Interactive mode: print warning and ask for confirmation
        eprintln!(
            "Warning: claw is running from a very broad directory ({}).\n\
             The agent can read and search everything under this path.\n\
             Consider running from inside your project: cd /path/to/project && claw",
            cwd.display()
        );
        eprint!("Continue anyway? [y/N]: ");
        io::stderr().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_lowercase();
        if trimmed != "y" && trimmed != "yes" {
            eprintln!("Aborted.");
            std::process::exit(0);
        }
        Ok(())
    } else {
        // Non-interactive mode: exit with error (JSON or text)
        let message = format!(
            "claw is running from a very broad directory ({}). \
             The agent can read and search everything under this path. \
             Use --allow-broad-cwd to proceed anyway, \
             or run from inside your project: cd /path/to/project && claw",
            cwd.display()
        );
        match output_format {
            CliOutputFormat::Json => {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "type": "error",
                        "error": message,
                    })
                );
            }
            CliOutputFormat::Text => {
                eprintln!("error: {message}");
            }
        }
        std::process::exit(1);
    }
}

pub(crate) fn run_stale_base_preflight(flag_value: Option<&str>) {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(_) => return,
    };
    let source = resolve_expected_base(flag_value, &cwd);
    let state = check_base_commit(&cwd, source.as_ref());
    if let Some(warning) = format_stale_base_warning(&state) {
        eprintln!("{warning}");
    }
}

pub(crate) fn run_repl(
    model: String,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    base_commit: Option<String>,
    reasoning_effort: Option<String>,
    allow_broad_cwd: bool,
    show_thinking: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    enforce_broad_cwd_policy(allow_broad_cwd, CliOutputFormat::Text)?;
    run_stale_base_preflight(base_commit.as_deref());
    let resolved_model = resolve_repl_model(model);
    let mut cli = LiveCli::new(resolved_model, true, allowed_tools, permission_mode)?;
    cli.set_reasoning_effort(reasoning_effort);
    if show_thinking {
        cli.set_show_thinking(true);
    }
    let mut editor =
        crate::input::LineEditor::new("> ", cli.repl_completion_candidates().unwrap_or_default());
    println!("{}", cli.startup_banner());
    println!("{}", format_connected_line(&cli.model));

    loop {
        editor.set_completions(cli.repl_completion_candidates().unwrap_or_default());
        match editor.read_line()? {
            crate::input::ReadOutcome::Submit(input) => {
                let trimmed = input.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }
                if matches!(trimmed.as_str(), "/exit" | "/quit") {
                    cli.persist_session()?;
                    break;
                }
                match SlashCommand::parse(&trimmed) {
                    Ok(Some(command)) => {
                        if cli.handle_repl_command(command)? {
                            cli.persist_session()?;
                        }
                        continue;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!("{error}");
                        continue;
                    }
                }
                // Bare-word skill dispatch: if the first token of the input
                // matches a known skill name, invoke it as `/skills <input>`
                // rather than forwarding raw text to the LLM (ROADMAP #36).
                let bare_first_token = trimmed.split_whitespace().next().unwrap_or_default();
                let looks_like_skill_name = !bare_first_token.is_empty()
                    && !bare_first_token.starts_with('/')
                    && bare_first_token
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
                if looks_like_skill_name {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    if let Ok(SkillSlashDispatch::Invoke(prompt)) =
                        resolve_skill_invocation(&cwd, Some(&trimmed))
                    {
                        editor.push_history(input);
                        cli.record_prompt_history(&trimmed);
                        cli.run_turn(&prompt)?;
                        continue;
                    }
                }
                editor.push_history(input);
                cli.record_prompt_history(&trimmed);
                cli.run_turn(&trimmed)?;
            }
            crate::input::ReadOutcome::Cancel => {}
            crate::input::ReadOutcome::Exit => {
                cli.persist_session()?;
                break;
            }
        }
    }

    Ok(())
}
