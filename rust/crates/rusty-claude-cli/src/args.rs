//! Argument parsing for the ACE CLI.
//!
//! This module owns all CLI argument parsing, action dispatch types, model/config
//! resolution helpers, and the suggestion/error-formatting utilities that support
//! them.  It was extracted from the monolithic `main.rs` as Phase-0 Task 0.1.

#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    clippy::unneeded_struct_pattern,
    clippy::unnecessary_wraps,
    clippy::unused_self
)]

use std::collections::BTreeSet;
use std::env;
use std::io::IsTerminal;
use std::path::PathBuf;

use api::detect_provider_kind;
use api::ProviderKind;
use commands::{
    classify_skills_slash_command, slash_command_specs, validate_slash_command_input,
    SkillSlashDispatch, SlashCommand,
};
use runtime::{ConfigLoader, PermissionMode, ResolvedPermissionMode};
use tools::GlobalToolRegistry;

// ── Re-export DEFAULT_MODEL from main so callers can reference it via args:: ──
// (the constant itself lives in main.rs)

pub(crate) type AllowedToolSet = BTreeSet<String>;

pub(crate) const CLI_OPTION_SUGGESTIONS: &[&str] = &[
    "--help",
    "-h",
    "--version",
    "-V",
    "--model",
    "--output-format",
    "--permission-mode",
    "--dangerously-skip-permissions",
    "--allowedTools",
    "--allowed-tools",
    "--resume",
    "--print",
    "--compact",
    "--base-commit",
    "-p",
];

// ─────────────────────────────────────────────────────────────────────────────
// Action / format enums
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliAction {
    DumpManifests {
        output_format: CliOutputFormat,
    },
    BootstrapPlan {
        output_format: CliOutputFormat,
    },
    Agents {
        args: Option<String>,
        output_format: CliOutputFormat,
    },
    Mcp {
        args: Option<String>,
        output_format: CliOutputFormat,
    },
    Skills {
        args: Option<String>,
        output_format: CliOutputFormat,
    },
    Plugins {
        action: Option<String>,
        target: Option<String>,
        output_format: CliOutputFormat,
    },
    PrintSystemPrompt {
        cwd: PathBuf,
        date: String,
        output_format: CliOutputFormat,
    },
    Version {
        output_format: CliOutputFormat,
    },
    ResumeSession {
        session_path: PathBuf,
        commands: Vec<String>,
        output_format: CliOutputFormat,
    },
    Status {
        model: String,
        permission_mode: PermissionMode,
        output_format: CliOutputFormat,
    },
    Sandbox {
        output_format: CliOutputFormat,
    },
    Prompt {
        prompt: String,
        model: String,
        output_format: CliOutputFormat,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
        compact: bool,
        base_commit: Option<String>,
        reasoning_effort: Option<String>,
        allow_broad_cwd: bool,
    },
    Login {
        output_format: CliOutputFormat,
    },
    Logout {
        output_format: CliOutputFormat,
    },
    Doctor {
        output_format: CliOutputFormat,
    },
    State {
        output_format: CliOutputFormat,
    },
    Init {
        output_format: CliOutputFormat,
    },
    Export {
        session_reference: String,
        output_path: Option<PathBuf>,
        output_format: CliOutputFormat,
    },
    Repl {
        model: String,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
        base_commit: Option<String>,
        reasoning_effort: Option<String>,
        allow_broad_cwd: bool,
        show_thinking: bool,
        tui: bool,
    },
    HelpTopic(LocalHelpTopic),
    // prompt-mode formatting is only supported for non-interactive runs
    Help {
        output_format: CliOutputFormat,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalHelpTopic {
    Status,
    Sandbox,
    Doctor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CliOutputFormat {
    Text,
    Json,
}

impl CliOutputFormat {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => Err(format!(
                "unsupported value for --output-format: {other} (expected text or json)"
            )),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Main entry point
// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
pub(crate) fn parse_args(args: &[String]) -> Result<CliAction, String> {
    let mut model = crate::DEFAULT_MODEL.to_string();
    let mut output_format = CliOutputFormat::Text;
    let mut permission_mode_override = None;
    let mut wants_help = false;
    let mut wants_version = false;
    let mut allowed_tool_values = Vec::new();
    let mut compact = false;
    let mut base_commit: Option<String> = None;
    let mut reasoning_effort: Option<String> = None;
    let mut allow_broad_cwd = false;
    let mut show_thinking = false;
    let mut tui_mode = false;
    let mut rest: Vec<String> = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" if rest.is_empty() => {
                wants_help = true;
                index += 1;
            }
            "--help" | "-h"
                if !rest.is_empty()
                    && matches!(
                        rest[0].as_str(),
                        "prompt"
                            | "login"
                            | "logout"
                            | "version"
                            | "state"
                            | "init"
                            | "export"
                            | "commit"
                            | "pr"
                            | "issue"
                    ) =>
            {
                // `--help` following a subcommand that would otherwise forward
                // the arg to the API (e.g. `claw prompt --help`) should show
                // top-level help instead. Subcommands that consume their own
                // args (agents, mcp, plugins, skills) and local help-topic
                // subcommands (status, sandbox, doctor) must NOT be intercepted
                // here — they handle --help in their own dispatch paths.
                wants_help = true;
                index += 1;
            }
            "--version" | "-V" => {
                wants_version = true;
                index += 1;
            }
            "--model" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --model".to_string())?;
                model = resolve_model_alias_with_config(value);
                index += 2;
            }
            flag if flag.starts_with("--model=") => {
                model = resolve_model_alias_with_config(&flag[8..]);
                index += 1;
            }
            "--output-format" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --output-format".to_string())?;
                output_format = CliOutputFormat::parse(value)?;
                index += 2;
            }
            "--permission-mode" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --permission-mode".to_string())?;
                permission_mode_override = Some(parse_permission_mode_arg(value)?);
                index += 2;
            }
            flag if flag.starts_with("--output-format=") => {
                output_format = CliOutputFormat::parse(&flag[16..])?;
                index += 1;
            }
            flag if flag.starts_with("--permission-mode=") => {
                permission_mode_override = Some(parse_permission_mode_arg(&flag[18..])?);
                index += 1;
            }
            "--dangerously-skip-permissions" => {
                permission_mode_override = Some(PermissionMode::DangerFullAccess);
                index += 1;
            }
            "--compact" => {
                compact = true;
                index += 1;
            }
            "--base-commit" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --base-commit".to_string())?;
                base_commit = Some(value.clone());
                index += 2;
            }
            flag if flag.starts_with("--base-commit=") => {
                base_commit = Some(flag[14..].to_string());
                index += 1;
            }
            "--reasoning-effort" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --reasoning-effort".to_string())?;
                if !matches!(value.as_str(), "low" | "medium" | "high") {
                    return Err(format!(
                        "invalid value for --reasoning-effort: '{value}'; must be low, medium, or high"
                    ));
                }
                reasoning_effort = Some(value.clone());
                index += 2;
            }
            flag if flag.starts_with("--reasoning-effort=") => {
                let value = &flag[19..];
                if !matches!(value, "low" | "medium" | "high") {
                    return Err(format!(
                        "invalid value for --reasoning-effort: '{value}'; must be low, medium, or high"
                    ));
                }
                reasoning_effort = Some(value.to_string());
                index += 1;
            }
            "--allow-broad-cwd" => {
                allow_broad_cwd = true;
                index += 1;
            }
            "--tui" => {
                tui_mode = true;
                index += 1;
            }
            "--show-thinking" => {
                show_thinking = true;
                index += 1;
            }
            "-p" => {
                // ACE CLI compat: -p "prompt" = one-shot prompt
                let prompt = args[index + 1..].join(" ");
                if prompt.trim().is_empty() {
                    return Err("-p requires a prompt string".to_string());
                }
                return Ok(CliAction::Prompt {
                    prompt,
                    model: resolve_model_alias_with_config(&model),
                    output_format,
                    allowed_tools: normalize_allowed_tools(&allowed_tool_values)?,
                    permission_mode: permission_mode_override
                        .unwrap_or_else(default_permission_mode),
                    compact,
                    base_commit: base_commit.clone(),
                    reasoning_effort: reasoning_effort.clone(),
                    allow_broad_cwd,
                });
            }
            "--print" => {
                // ACE CLI compat: --print makes output non-interactive
                output_format = CliOutputFormat::Text;
                index += 1;
            }
            "--resume" if rest.is_empty() => {
                rest.push("--resume".to_string());
                index += 1;
            }
            flag if rest.is_empty() && flag.starts_with("--resume=") => {
                rest.push("--resume".to_string());
                rest.push(flag[9..].to_string());
                index += 1;
            }
            "--allowedTools" | "--allowed-tools" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --allowedTools".to_string())?;
                allowed_tool_values.push(value.clone());
                index += 2;
            }
            flag if flag.starts_with("--allowedTools=") => {
                allowed_tool_values.push(flag[15..].to_string());
                index += 1;
            }
            flag if flag.starts_with("--allowed-tools=") => {
                allowed_tool_values.push(flag[16..].to_string());
                index += 1;
            }
            other if rest.is_empty() && other.starts_with('-') => {
                return Err(format_unknown_option(other))
            }
            other => {
                rest.push(other.to_string());
                index += 1;
            }
        }
    }

    if wants_help {
        return Ok(CliAction::Help { output_format });
    }

    if wants_version {
        return Ok(CliAction::Version { output_format });
    }

    let allowed_tools = normalize_allowed_tools(&allowed_tool_values)?;

    if rest.is_empty() {
        let permission_mode = permission_mode_override.unwrap_or_else(default_permission_mode);
        // When stdin is not a terminal (pipe/redirect) and no prompt is given on the
        // command line, read stdin as the prompt and dispatch as a one-shot Prompt
        // rather than starting the interactive REPL (which would consume the pipe and
        // print the startup banner, then exit without sending anything to the API).
        if !std::io::stdin().is_terminal() {
            use std::io::IsTerminal;
            let mut buf = String::new();
            let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf);
            let piped = buf.trim().to_string();
            if !piped.is_empty() {
                return Ok(CliAction::Prompt {
                    model,
                    prompt: piped,
                    allowed_tools,
                    permission_mode,
                    output_format,
                    compact: false,
                    base_commit,
                    reasoning_effort,
                    allow_broad_cwd,
                });
            }
        }
        return Ok(CliAction::Repl {
            model,
            allowed_tools,
            permission_mode,
            base_commit,
            reasoning_effort: reasoning_effort.clone(),
            allow_broad_cwd,
            show_thinking,
            tui: tui_mode,
        });
    }
    if rest.first().map(String::as_str) == Some("--resume") {
        return parse_resume_args(&rest[1..], output_format);
    }
    if let Some(action) = parse_local_help_action(&rest) {
        return action;
    }
    if let Some(action) =
        parse_single_word_command_alias(&rest, &model, permission_mode_override, output_format)
    {
        return action;
    }

    let permission_mode = permission_mode_override.unwrap_or_else(default_permission_mode);

    match rest[0].as_str() {
        "dump-manifests" => Ok(CliAction::DumpManifests { output_format }),
        "bootstrap-plan" => Ok(CliAction::BootstrapPlan { output_format }),
        "agents" => Ok(CliAction::Agents {
            args: join_optional_args(&rest[1..]),
            output_format,
        }),
        "mcp" => Ok(CliAction::Mcp {
            args: join_optional_args(&rest[1..]),
            output_format,
        }),
        "skills" => {
            let args = join_optional_args(&rest[1..]);
            match classify_skills_slash_command(args.as_deref()) {
                SkillSlashDispatch::Invoke(prompt) => Ok(CliAction::Prompt {
                    prompt,
                    model,
                    output_format,
                    allowed_tools,
                    permission_mode,
                    compact,
                    base_commit,
                    reasoning_effort: reasoning_effort.clone(),
                    allow_broad_cwd,
                }),
                SkillSlashDispatch::Local => Ok(CliAction::Skills {
                    args,
                    output_format,
                }),
            }
        }
        "system-prompt" => parse_system_prompt_args(&rest[1..], output_format),
        "login" => Ok(CliAction::Login { output_format }),
        "logout" => Ok(CliAction::Logout { output_format }),
        "init" => Ok(CliAction::Init { output_format }),
        "export" => parse_export_args(&rest[1..], output_format),
        "prompt" => {
            let prompt = rest[1..].join(" ");
            if prompt.trim().is_empty() {
                return Err("prompt subcommand requires a prompt string".to_string());
            }
            Ok(CliAction::Prompt {
                prompt,
                model,
                output_format,
                allowed_tools,
                permission_mode,
                compact,
                base_commit: base_commit.clone(),
                reasoning_effort: reasoning_effort.clone(),
                allow_broad_cwd,
            })
        }
        other if other.starts_with('/') => parse_direct_slash_cli_action(
            &rest,
            model,
            output_format,
            allowed_tools,
            permission_mode,
            compact,
            base_commit,
            reasoning_effort,
            allow_broad_cwd,
        ),
        _other => Ok(CliAction::Prompt {
            prompt: rest.join(" "),
            model,
            output_format,
            allowed_tools,
            permission_mode,
            compact,
            base_commit,
            reasoning_effort: reasoning_effort.clone(),
            allow_broad_cwd,
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Argument helpers
// ─────────────────────────────────────────────────────────────────────────────

fn parse_local_help_action(rest: &[String]) -> Option<Result<CliAction, String>> {
    if rest.len() != 2 || !is_help_flag(&rest[1]) {
        return None;
    }

    let topic = match rest[0].as_str() {
        "status" => LocalHelpTopic::Status,
        "sandbox" => LocalHelpTopic::Sandbox,
        "doctor" => LocalHelpTopic::Doctor,
        _ => return None,
    };
    Some(Ok(CliAction::HelpTopic(topic)))
}

fn is_help_flag(value: &str) -> bool {
    matches!(value, "--help" | "-h")
}

fn parse_single_word_command_alias(
    rest: &[String],
    model: &str,
    permission_mode_override: Option<PermissionMode>,
    output_format: CliOutputFormat,
) -> Option<Result<CliAction, String>> {
    if rest.len() != 1 {
        return None;
    }

    match rest[0].as_str() {
        "help" => Some(Ok(CliAction::Help { output_format })),
        "version" => Some(Ok(CliAction::Version { output_format })),
        "status" => Some(Ok(CliAction::Status {
            model: model.to_string(),
            permission_mode: permission_mode_override.unwrap_or_else(default_permission_mode),
            output_format,
        })),
        "sandbox" => Some(Ok(CliAction::Sandbox { output_format })),
        "doctor" => Some(Ok(CliAction::Doctor { output_format })),
        "state" => Some(Ok(CliAction::State { output_format })),
        other => bare_slash_command_guidance(other).map(Err),
    }
}

fn bare_slash_command_guidance(command_name: &str) -> Option<String> {
    if matches!(
        command_name,
        "dump-manifests"
            | "bootstrap-plan"
            | "agents"
            | "mcp"
            | "skills"
            | "system-prompt"
            | "login"
            | "logout"
            | "init"
            | "prompt"
            | "export"
    ) {
        return None;
    }
    let slash_command = slash_command_specs()
        .iter()
        .find(|spec| spec.name == command_name)?;
    let guidance = if slash_command.resume_supported {
        format!(
            "`ace {command_name}` is a slash command. Use `ace --resume SESSION.jsonl /{command_name}` or start `ace` and run `/{command_name}`."
        )
    } else {
        format!(
            "`ace {command_name}` is a slash command. Start `ace` and run `/{command_name}` inside the REPL."
        )
    };
    Some(guidance)
}

pub(crate) fn join_optional_args(args: &[String]) -> Option<String> {
    let joined = args.join(" ");
    let trimmed = joined.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn parse_direct_slash_cli_action(
    rest: &[String],
    model: String,
    output_format: CliOutputFormat,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    compact: bool,
    base_commit: Option<String>,
    reasoning_effort: Option<String>,
    allow_broad_cwd: bool,
) -> Result<CliAction, String> {
    let raw = rest.join(" ");
    match SlashCommand::parse(&raw) {
        Ok(Some(SlashCommand::Help)) => Ok(CliAction::Help { output_format }),
        Ok(Some(SlashCommand::Agents { args })) => Ok(CliAction::Agents {
            args,
            output_format,
        }),
        Ok(Some(SlashCommand::Mcp { action, target })) => Ok(CliAction::Mcp {
            args: match (action, target) {
                (None, None) => None,
                (Some(action), None) => Some(action),
                (Some(action), Some(target)) => Some(format!("{action} {target}")),
                (None, Some(target)) => Some(target),
            },
            output_format,
        }),
        Ok(Some(SlashCommand::Skills { args })) => {
            match classify_skills_slash_command(args.as_deref()) {
                SkillSlashDispatch::Invoke(prompt) => Ok(CliAction::Prompt {
                    prompt,
                    model,
                    output_format,
                    allowed_tools,
                    permission_mode,
                    compact,
                    base_commit,
                    reasoning_effort: reasoning_effort.clone(),
                    allow_broad_cwd,
                }),
                SkillSlashDispatch::Local => Ok(CliAction::Skills {
                    args,
                    output_format,
                }),
            }
        }
        Ok(Some(SlashCommand::Unknown(name))) => Err(format_unknown_direct_slash_command(&name)),
        Ok(Some(command)) => Err({
            let _ = command;
            format!(
                "slash command {command_name} is interactive-only. Start `ace` and run it there, or use `ace --resume SESSION.jsonl {command_name}` / `ace --resume {latest} {command_name}` when the command is marked [resume] in /help.",
                command_name = rest[0],
                latest = crate::LATEST_SESSION_REFERENCE,
            )
        }),
        Ok(None) => Err(format!("unknown subcommand: {}", rest[0])),
        Err(error) => Err(error.to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Error / suggestion formatting
// ─────────────────────────────────────────────────────────────────────────────

fn format_unknown_option(option: &str) -> String {
    let mut message = format!("unknown option: {option}");
    if let Some(suggestion) = suggest_closest_term(option, CLI_OPTION_SUGGESTIONS) {
        message.push_str("\nDid you mean ");
        message.push_str(suggestion);
        message.push('?');
    }
    message.push_str("\nRun `ace --help` for usage.");
    message
}

fn format_unknown_direct_slash_command(name: &str) -> String {
    let mut message = format!("unknown slash command outside the REPL: /{name}");
    if let Some(suggestions) = render_suggestion_line("Did you mean", &suggest_slash_commands(name))
    {
        message.push('\n');
        message.push_str(&suggestions);
    }
    if let Some(note) = omc_compatibility_note_for_unknown_slash_command(name) {
        message.push('\n');
        message.push_str(note);
    }
    message.push_str("\nRun `ace --help` for CLI usage, or start `ace` and use /help.");
    message
}

pub(crate) fn format_unknown_slash_command(name: &str) -> String {
    let mut message = format!("Unknown slash command: /{name}");
    if let Some(suggestions) = render_suggestion_line("Did you mean", &suggest_slash_commands(name))
    {
        message.push('\n');
        message.push_str(&suggestions);
    }
    if let Some(note) = omc_compatibility_note_for_unknown_slash_command(name) {
        message.push('\n');
        message.push_str(note);
    }
    message.push_str("\n  Help             /help lists available slash commands");
    message
}

fn omc_compatibility_note_for_unknown_slash_command(name: &str) -> Option<&'static str> {
    name.starts_with("oh-my-claudecode:")
        .then_some(
            "Compatibility note: `/oh-my-claudecode:*` is a Claude Code/OMC plugin command. `ace` does not yet load plugin slash commands, Claude statusline stdin, or OMC session hooks.",
        )
}

fn render_suggestion_line(label: &str, suggestions: &[String]) -> Option<String> {
    (!suggestions.is_empty()).then(|| format!("  {label:<16} {}", suggestions.join(", "),))
}

fn suggest_slash_commands(input: &str) -> Vec<String> {
    let mut candidates = slash_command_specs()
        .iter()
        .flat_map(|spec| {
            std::iter::once(spec.name)
                .chain(spec.aliases.iter().copied())
                .map(|name| format!("/{name}"))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();
    let candidate_refs = candidates.iter().map(String::as_str).collect::<Vec<_>>();
    ranked_suggestions(input.trim_start_matches('/'), &candidate_refs)
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn suggest_closest_term<'a>(input: &str, candidates: &'a [&'a str]) -> Option<&'a str> {
    ranked_suggestions(input, candidates).into_iter().next()
}

fn ranked_suggestions<'a>(input: &str, candidates: &'a [&'a str]) -> Vec<&'a str> {
    let normalized_input = input.trim_start_matches('/').to_ascii_lowercase();
    let mut ranked = candidates
        .iter()
        .filter_map(|candidate| {
            let normalized_candidate = candidate.trim_start_matches('/').to_ascii_lowercase();
            let distance = levenshtein_distance(&normalized_input, &normalized_candidate);
            let prefix_bonus = usize::from(
                !(normalized_candidate.starts_with(&normalized_input)
                    || normalized_input.starts_with(&normalized_candidate)),
            );
            let score = distance + prefix_bonus;
            (score <= 4).then_some((score, *candidate))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| left.cmp(right).then_with(|| left.1.cmp(right.1)));
    ranked
        .into_iter()
        .map(|(_, candidate)| candidate)
        .take(3)
        .collect()
}

fn levenshtein_distance(left: &str, right: &str) -> usize {
    if left.is_empty() {
        return right.chars().count();
    }
    if right.is_empty() {
        return left.chars().count();
    }

    let right_chars = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; right_chars.len() + 1];

    for (left_index, left_char) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right_chars.iter().enumerate() {
            let substitution_cost = usize::from(left_char != *right_char);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + substitution_cost);
        }
        previous.clone_from(&current);
    }

    previous[right_chars.len()]
}

// ─────────────────────────────────────────────────────────────────────────────
// Model / config resolution
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn resolve_model_alias(model: &str) -> &str {
    match model {
        "opus" => "claude-opus-4-6",
        "sonnet" => "claude-sonnet-4-6",
        "haiku" => "claude-haiku-4-5-20251213",
        _ => model,
    }
}

/// Resolve a model name through user-defined config aliases first, then fall
/// back to the built-in alias table. This is the entry point used wherever a
/// user-supplied model string is about to be dispatched to a provider.
pub(crate) fn resolve_model_alias_with_config(model: &str) -> String {
    let trimmed = model.trim();
    if let Some(resolved) = config_alias_for_current_dir(trimmed) {
        return resolve_model_alias(&resolved).to_string();
    }
    resolve_model_alias(trimmed).to_string()
}

fn config_alias_for_current_dir(alias: &str) -> Option<String> {
    if alias.is_empty() {
        return None;
    }
    let cwd = env::current_dir().ok()?;
    let loader = ConfigLoader::default_for(&cwd);
    let config = loader.load().ok()?;
    config.aliases().get(alias).cloned()
}

pub(crate) fn normalize_allowed_tools(values: &[String]) -> Result<Option<AllowedToolSet>, String> {
    if values.is_empty() {
        return Ok(None);
    }
    current_tool_registry()?.normalize_allowed_tools(values)
}

fn current_tool_registry() -> Result<GlobalToolRegistry, String> {
    let cwd = env::current_dir().map_err(|error| error.to_string())?;
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = loader.load().map_err(|error| error.to_string())?;
    let state = crate::build_runtime_plugin_state_with_loader(&cwd, &loader, &runtime_config)
        .map_err(|error| error.to_string())?;
    let registry = state.tool_registry.clone();
    if let Some(mcp_state) = state.mcp_state {
        mcp_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shutdown()
            .map_err(|error| error.to_string())?;
    }
    Ok(registry)
}

pub(crate) fn parse_permission_mode_arg(value: &str) -> Result<PermissionMode, String> {
    normalize_permission_mode(value)
        .ok_or_else(|| {
            format!(
                "unsupported permission mode '{value}'. Use read-only, workspace-write, or danger-full-access."
            )
        })
        .map(permission_mode_from_label)
}

pub(crate) fn permission_mode_from_label(mode: &str) -> PermissionMode {
    match mode {
        "read-only" => PermissionMode::ReadOnly,
        "workspace-write" => PermissionMode::WorkspaceWrite,
        "danger-full-access" => PermissionMode::DangerFullAccess,
        other => panic!("unsupported permission mode label: {other}"),
    }
}

pub(crate) fn permission_mode_from_resolved(mode: ResolvedPermissionMode) -> PermissionMode {
    match mode {
        ResolvedPermissionMode::ReadOnly => PermissionMode::ReadOnly,
        ResolvedPermissionMode::WorkspaceWrite => PermissionMode::WorkspaceWrite,
        ResolvedPermissionMode::DangerFullAccess => PermissionMode::DangerFullAccess,
    }
}

pub(crate) fn default_permission_mode() -> PermissionMode {
    env::var("RUSTY_CLAUDE_PERMISSION_MODE")
        .ok()
        .as_deref()
        .and_then(normalize_permission_mode)
        .map(permission_mode_from_label)
        .or_else(config_permission_mode_for_current_dir)
        .unwrap_or(PermissionMode::DangerFullAccess)
}

pub(crate) fn config_permission_mode_for_current_dir() -> Option<PermissionMode> {
    let cwd = env::current_dir().ok()?;
    let loader = ConfigLoader::default_for(&cwd);
    loader
        .load()
        .ok()?
        .permission_mode()
        .map(permission_mode_from_resolved)
}

pub(crate) fn config_model_for_current_dir() -> Option<String> {
    let cwd = env::current_dir().ok()?;
    let loader = ConfigLoader::default_for(&cwd);
    loader.load().ok()?.model().map(ToOwned::to_owned)
}

pub(crate) fn resolve_repl_model(cli_model: String) -> String {
    if cli_model != crate::DEFAULT_MODEL {
        return cli_model;
    }
    if let Some(env_model) = env::var("ANTHROPIC_MODEL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return resolve_model_alias_with_config(&env_model);
    }
    if let Some(config_model) = config_model_for_current_dir() {
        return resolve_model_alias_with_config(&config_model);
    }
    cli_model
}

pub(crate) fn provider_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::Xai => "xai",
        ProviderKind::OpenAi => "openai",
        ProviderKind::Gemini => "gemini",
        ProviderKind::Ollama => "ollama",
    }
}

pub(crate) fn format_connected_line(model: &str) -> String {
    let provider = provider_label(detect_provider_kind(model));
    format!("Connected: {model} via {provider}")
}

pub(crate) fn filter_tool_specs(
    tool_registry: &GlobalToolRegistry,
    allowed_tools: Option<&AllowedToolSet>,
) -> Vec<api::ToolDefinition> {
    tool_registry.definitions(allowed_tools)
}

pub(crate) fn parse_system_prompt_args(
    args: &[String],
    output_format: CliOutputFormat,
) -> Result<CliAction, String> {
    let mut cwd = env::current_dir().map_err(|error| error.to_string())?;
    let mut date = crate::DEFAULT_DATE.to_string();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--cwd" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --cwd".to_string())?;
                cwd = PathBuf::from(value);
                index += 2;
            }
            "--date" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --date".to_string())?;
                date.clone_from(value);
                index += 2;
            }
            other => return Err(format!("unknown system-prompt option: {other}")),
        }
    }

    Ok(CliAction::PrintSystemPrompt {
        cwd,
        date,
        output_format,
    })
}

/// Normalize a raw permission-mode string to one of the three canonical labels,
/// or return `None` if the string is unrecognised.
pub(crate) fn normalize_permission_mode(mode: &str) -> Option<&'static str> {
    match mode.trim() {
        "read-only" => Some("read-only"),
        "workspace-write" => Some("workspace-write"),
        "danger-full-access" => Some("danger-full-access"),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Export / resume parsing
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn parse_export_args(
    args: &[String],
    output_format: CliOutputFormat,
) -> Result<CliAction, String> {
    let mut session_reference = crate::LATEST_SESSION_REFERENCE.to_string();
    let mut output_path: Option<PathBuf> = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--session" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --session".to_string())?;
                session_reference = value.clone();
                index += 2;
            }
            flag if flag.starts_with("--session=") => {
                session_reference = flag[10..].to_string();
                index += 1;
            }
            "--output" | "-o" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for {}", args[index]))?;
                output_path = Some(PathBuf::from(value));
                index += 2;
            }
            flag if flag.starts_with("--output=") => {
                output_path = Some(PathBuf::from(&flag[9..]));
                index += 1;
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown export option: {other}"));
            }
            other if output_path.is_none() => {
                output_path = Some(PathBuf::from(other));
                index += 1;
            }
            other => {
                return Err(format!("unexpected export argument: {other}"));
            }
        }
    }

    Ok(CliAction::Export {
        session_reference,
        output_path,
        output_format,
    })
}

pub(crate) fn parse_resume_args(
    args: &[String],
    output_format: CliOutputFormat,
) -> Result<CliAction, String> {
    let (session_path, command_tokens): (PathBuf, &[String]) = match args.first() {
        None => (PathBuf::from(crate::LATEST_SESSION_REFERENCE), &[]),
        Some(first) if looks_like_slash_command_token(first) => {
            (PathBuf::from(crate::LATEST_SESSION_REFERENCE), args)
        }
        Some(first) => (PathBuf::from(first), &args[1..]),
    };
    let mut commands = Vec::new();
    let mut current_command = String::new();

    for token in command_tokens {
        if token.trim_start().starts_with('/') {
            if resume_command_can_absorb_token(&current_command, token) {
                current_command.push(' ');
                current_command.push_str(token);
                continue;
            }
            if !current_command.is_empty() {
                commands.push(current_command);
            }
            current_command = String::from(token.as_str());
            continue;
        }

        if current_command.is_empty() {
            return Err("--resume trailing arguments must be slash commands".to_string());
        }

        current_command.push(' ');
        current_command.push_str(token);
    }

    if !current_command.is_empty() {
        commands.push(current_command);
    }

    Ok(CliAction::ResumeSession {
        session_path,
        commands,
        output_format,
    })
}

fn resume_command_can_absorb_token(current_command: &str, token: &str) -> bool {
    matches!(
        SlashCommand::parse(current_command),
        Ok(Some(SlashCommand::Export { path: None }))
    ) && !looks_like_slash_command_token(token)
}

fn looks_like_slash_command_token(token: &str) -> bool {
    let trimmed = token.trim_start();
    let Some(name) = trimmed.strip_prefix('/').and_then(|value| {
        value
            .split_whitespace()
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }) else {
        return false;
    };

    slash_command_specs()
        .iter()
        .any(|spec| spec.name == name || spec.aliases.contains(&name))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        default_permission_mode, filter_tool_specs, format_connected_line,
        format_unknown_slash_command, normalize_permission_mode, parse_args, parse_export_args,
        resolve_model_alias, resolve_model_alias_with_config, resolve_repl_model, CliAction,
        CliOutputFormat, LocalHelpTopic,
    };
    use plugins::{PluginTool, PluginToolDefinition, PluginToolPermission};
    use runtime::{ConfigLoader, PermissionMode};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use tools::GlobalToolRegistry;

    use crate::{DEFAULT_MODEL, LATEST_SESSION_REFERENCE};

    fn registry_with_plugin_tool() -> GlobalToolRegistry {
        GlobalToolRegistry::with_plugin_tools(vec![PluginTool::new(
            "plugin-demo@external",
            "plugin-demo",
            PluginToolDefinition {
                name: "plugin_echo".to_string(),
                description: Some("Echo plugin payload".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    },
                    "required": ["message"],
                    "additionalProperties": false
                }),
            },
            "echo".to_string(),
            Vec::new(),
            PluginToolPermission::WorkspaceWrite,
            None,
        )])
        .expect("plugin tool registry should build")
    }

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("rusty-claude-cli-args-{nanos}-{unique}"))
    }

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn with_current_dir<T>(cwd: &std::path::Path, f: impl FnOnce() -> T) -> T {
        let _guard = cwd_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::env::current_dir().expect("cwd should load");
        std::env::set_current_dir(cwd).expect("cwd should change");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        std::env::set_current_dir(previous).expect("cwd should restore");
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    // ── Helper used by the format_unknown_slash_command tests ─────────────────
    #[cfg(test)]
    fn format_unknown_slash_command_message(name: &str) -> String {
        let suggestions = super::suggest_slash_commands(name);
        let mut message = format!("unknown slash command: /{name}.");
        if !suggestions.is_empty() {
            message.push_str(" Did you mean ");
            message.push_str(&suggestions.join(", "));
            message.push('?');
        }
        if let Some(note) = super::omc_compatibility_note_for_unknown_slash_command(name) {
            message.push(' ');
            message.push_str(note);
        }
        message.push_str(" Use /help to list available commands.");
        message
    }

    // ─── parse_args tests ─────────────────────────────────────────────────────

    #[test]
    fn defaults_to_repl_when_no_args() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        assert_eq!(
            parse_args(&[]).expect("args should parse"),
            CliAction::Repl {
                model: DEFAULT_MODEL.to_string(),
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
                show_thinking: false,
                tui: false,
            }
        );
    }

    #[test]
    fn default_permission_mode_uses_project_config_when_env_is_unset() {
        let _guard = env_lock();
        let root = temp_dir();
        let cwd = root.join("project");
        let config_home = root.join("config-home");
        std::fs::create_dir_all(cwd.join(".claw")).expect("project config dir should exist");
        std::fs::create_dir_all(&config_home).expect("config home should exist");
        std::fs::write(
            cwd.join(".claw").join("settings.json"),
            r#"{"permissionMode":"acceptEdits"}"#,
        )
        .expect("project config should write");

        let original_config_home = std::env::var("CLAW_CONFIG_HOME").ok();
        let original_permission_mode = std::env::var("RUSTY_CLAUDE_PERMISSION_MODE").ok();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");

        let resolved = with_current_dir(&cwd, default_permission_mode);

        match original_config_home {
            Some(value) => std::env::set_var("CLAW_CONFIG_HOME", value),
            None => std::env::remove_var("CLAW_CONFIG_HOME"),
        }
        match original_permission_mode {
            Some(value) => std::env::set_var("RUSTY_CLAUDE_PERMISSION_MODE", value),
            None => std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE"),
        }
        std::fs::remove_dir_all(root).expect("temp config root should clean up");

        assert_eq!(resolved, PermissionMode::WorkspaceWrite);
    }

    #[test]
    fn env_permission_mode_overrides_project_config_default() {
        let _guard = env_lock();
        let root = temp_dir();
        let cwd = root.join("project");
        let config_home = root.join("config-home");
        std::fs::create_dir_all(cwd.join(".claw")).expect("project config dir should exist");
        std::fs::create_dir_all(&config_home).expect("config home should exist");
        std::fs::write(
            cwd.join(".claw").join("settings.json"),
            r#"{"permissionMode":"acceptEdits"}"#,
        )
        .expect("project config should write");

        let original_config_home = std::env::var("CLAW_CONFIG_HOME").ok();
        let original_permission_mode = std::env::var("RUSTY_CLAUDE_PERMISSION_MODE").ok();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::set_var("RUSTY_CLAUDE_PERMISSION_MODE", "read-only");

        let resolved = with_current_dir(&cwd, default_permission_mode);

        match original_config_home {
            Some(value) => std::env::set_var("CLAW_CONFIG_HOME", value),
            None => std::env::remove_var("CLAW_CONFIG_HOME"),
        }
        match original_permission_mode {
            Some(value) => std::env::set_var("RUSTY_CLAUDE_PERMISSION_MODE", value),
            None => std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE"),
        }
        std::fs::remove_dir_all(root).expect("temp config root should clean up");

        assert_eq!(resolved, PermissionMode::ReadOnly);
    }

    #[test]
    fn parses_prompt_subcommand() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec![
            "prompt".to_string(),
            "hello".to_string(),
            "world".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Prompt {
                prompt: "hello world".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn parses_bare_prompt_and_json_output_flag() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec![
            "--output-format=json".to_string(),
            "--model".to_string(),
            "claude-opus".to_string(),
            "explain".to_string(),
            "this".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Prompt {
                prompt: "explain this".to_string(),
                model: "claude-opus".to_string(),
                output_format: CliOutputFormat::Json,
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn parses_compact_flag_for_prompt_mode() {
        // given a bare prompt invocation that includes the --compact flag
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec![
            "--compact".to_string(),
            "summarize".to_string(),
            "this".to_string(),
        ];

        // when parse_args interprets the flag
        let parsed = parse_args(&args).expect("args should parse");

        // then compact mode is propagated and other defaults stay unchanged
        assert_eq!(
            parsed,
            CliAction::Prompt {
                prompt: "summarize this".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
                compact: true,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn prompt_subcommand_defaults_compact_to_false() {
        // given a `prompt` subcommand invocation without --compact
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec!["prompt".to_string(), "hello".to_string()];

        // when parse_args runs
        let parsed = parse_args(&args).expect("args should parse");

        // then compact stays false (opt-in flag)
        match parsed {
            CliAction::Prompt { compact, .. } => assert!(!compact),
            other => panic!("expected Prompt action, got {other:?}"),
        }
    }

    #[test]
    fn resolves_model_aliases_in_args() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec![
            "--model".to_string(),
            "opus".to_string(),
            "explain".to_string(),
            "this".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Prompt {
                prompt: "explain this".to_string(),
                model: "claude-opus-4-6".to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn resolves_known_model_aliases() {
        assert_eq!(resolve_model_alias("opus"), "claude-opus-4-6");
        assert_eq!(resolve_model_alias("sonnet"), "claude-sonnet-4-6");
        assert_eq!(resolve_model_alias("haiku"), "claude-haiku-4-5-20251213");
        assert_eq!(resolve_model_alias("claude-opus"), "claude-opus");
    }

    #[test]
    fn user_defined_aliases_resolve_before_provider_dispatch() {
        // given
        let _guard = env_lock();
        let root = temp_dir();
        let cwd = root.join("project");
        let config_home = root.join("config-home");
        std::fs::create_dir_all(cwd.join(".claw")).expect("project config dir should exist");
        std::fs::create_dir_all(&config_home).expect("config home should exist");
        std::fs::write(
            cwd.join(".claw").join("settings.json"),
            r#"{"aliases":{"fast":"claude-haiku-4-5-20251213","smart":"opus","cheap":"grok-3-mini"}}"#,
        )
        .expect("project config should write");

        let original_config_home = std::env::var("CLAW_CONFIG_HOME").ok();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);

        // when
        let direct = with_current_dir(&cwd, || resolve_model_alias_with_config("fast"));
        let chained = with_current_dir(&cwd, || resolve_model_alias_with_config("smart"));
        let cross_provider = with_current_dir(&cwd, || resolve_model_alias_with_config("cheap"));
        let unknown = with_current_dir(&cwd, || resolve_model_alias_with_config("unknown-model"));
        let builtin = with_current_dir(&cwd, || resolve_model_alias_with_config("haiku"));

        match original_config_home {
            Some(value) => std::env::set_var("CLAW_CONFIG_HOME", value),
            None => std::env::remove_var("CLAW_CONFIG_HOME"),
        }
        std::fs::remove_dir_all(root).expect("temp config root should clean up");

        // then
        assert_eq!(direct, "claude-haiku-4-5-20251213");
        assert_eq!(chained, "claude-opus-4-6");
        assert_eq!(cross_provider, "grok-3-mini");
        assert_eq!(unknown, "unknown-model");
        assert_eq!(builtin, "claude-haiku-4-5-20251213");
    }

    #[test]
    fn parses_version_flags_without_initializing_prompt_mode() {
        assert_eq!(
            parse_args(&["--version".to_string()]).expect("args should parse"),
            CliAction::Version {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["-V".to_string()]).expect("args should parse"),
            CliAction::Version {
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_permission_mode_flag() {
        let args = vec!["--permission-mode=read-only".to_string()];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Repl {
                model: DEFAULT_MODEL.to_string(),
                allowed_tools: None,
                permission_mode: PermissionMode::ReadOnly,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
                show_thinking: false,
                tui: false,
            }
        );
    }

    #[test]
    fn dangerously_skip_permissions_flag_forces_danger_full_access_in_repl() {
        let _guard = env_lock();
        std::env::set_var("RUSTY_CLAUDE_PERMISSION_MODE", "read-only");
        let args = vec!["--dangerously-skip-permissions".to_string()];
        let parsed = parse_args(&args).expect("args should parse");
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");

        assert_eq!(
            parsed,
            CliAction::Repl {
                model: DEFAULT_MODEL.to_string(),
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
                show_thinking: false,
                tui: false,
            }
        );
    }

    #[test]
    fn dangerously_skip_permissions_flag_applies_to_prompt_subcommand() {
        let _guard = env_lock();
        std::env::set_var("RUSTY_CLAUDE_PERMISSION_MODE", "read-only");
        let args = vec![
            "--dangerously-skip-permissions".to_string(),
            "prompt".to_string(),
            "do".to_string(),
            "the".to_string(),
            "thing".to_string(),
        ];
        let parsed = parse_args(&args).expect("args should parse");
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");

        assert_eq!(
            parsed,
            CliAction::Prompt {
                prompt: "do the thing".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn parses_allowed_tools_flags_with_aliases_and_lists() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec![
            "--allowedTools".to_string(),
            "read,glob".to_string(),
            "--allowed-tools=write_file".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Repl {
                model: DEFAULT_MODEL.to_string(),
                allowed_tools: Some(
                    ["glob_search", "read_file", "write_file"]
                        .into_iter()
                        .map(str::to_string)
                        .collect()
                ),
                permission_mode: PermissionMode::DangerFullAccess,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
                show_thinking: false,
                tui: false,
            }
        );
    }

    #[test]
    fn rejects_unknown_allowed_tools() {
        let error = parse_args(&["--allowedTools".to_string(), "teleport".to_string()])
            .expect_err("tool should be rejected");
        assert!(error.contains("unsupported tool in --allowedTools: teleport"));
    }

    #[test]
    fn parses_system_prompt_options() {
        let args = vec![
            "system-prompt".to_string(),
            "--cwd".to_string(),
            "/tmp/project".to_string(),
            "--date".to_string(),
            "2026-04-01".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::PrintSystemPrompt {
                cwd: PathBuf::from("/tmp/project"),
                date: "2026-04-01".to_string(),
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_login_and_logout_subcommands() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        assert_eq!(
            parse_args(&["login".to_string()]).expect("login should parse"),
            CliAction::Login {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["logout".to_string()]).expect("logout should parse"),
            CliAction::Logout {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["doctor".to_string()]).expect("doctor should parse"),
            CliAction::Doctor {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["state".to_string()]).expect("state should parse"),
            CliAction::State {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&[
                "state".to_string(),
                "--output-format".to_string(),
                "json".to_string()
            ])
            .expect("state --output-format json should parse"),
            CliAction::State {
                output_format: CliOutputFormat::Json,
            }
        );
        assert_eq!(
            parse_args(&["init".to_string()]).expect("init should parse"),
            CliAction::Init {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["agents".to_string()]).expect("agents should parse"),
            CliAction::Agents {
                args: None,
                output_format: CliOutputFormat::Text
            }
        );
        assert_eq!(
            parse_args(&["mcp".to_string()]).expect("mcp should parse"),
            CliAction::Mcp {
                args: None,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["skills".to_string()]).expect("skills should parse"),
            CliAction::Skills {
                args: None,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&[
                "skills".to_string(),
                "help".to_string(),
                "overview".to_string()
            ])
            .expect("skills help overview should invoke"),
            CliAction::Prompt {
                prompt: "$help overview".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: crate::default_permission_mode(),
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
        assert_eq!(
            parse_args(&["agents".to_string(), "--help".to_string()])
                .expect("agents help should parse"),
            CliAction::Agents {
                args: Some("--help".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn local_command_help_flags_stay_on_the_local_parser_path() {
        assert_eq!(
            parse_args(&["status".to_string(), "--help".to_string()])
                .expect("status help should parse"),
            CliAction::HelpTopic(LocalHelpTopic::Status)
        );
        assert_eq!(
            parse_args(&["sandbox".to_string(), "-h".to_string()])
                .expect("sandbox help should parse"),
            CliAction::HelpTopic(LocalHelpTopic::Sandbox)
        );
        assert_eq!(
            parse_args(&["doctor".to_string(), "--help".to_string()])
                .expect("doctor help should parse"),
            CliAction::HelpTopic(LocalHelpTopic::Doctor)
        );
    }

    #[test]
    fn parses_single_word_command_aliases_without_falling_back_to_prompt_mode() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        assert_eq!(
            parse_args(&["help".to_string()]).expect("help should parse"),
            CliAction::Help {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["version".to_string()]).expect("version should parse"),
            CliAction::Version {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["status".to_string()]).expect("status should parse"),
            CliAction::Status {
                model: DEFAULT_MODEL.to_string(),
                permission_mode: PermissionMode::DangerFullAccess,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["sandbox".to_string()]).expect("sandbox should parse"),
            CliAction::Sandbox {
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_bare_export_subcommand_targeting_latest_session() {
        // given
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec!["export".to_string()];

        // when
        let parsed = parse_args(&args).expect("bare export should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: LATEST_SESSION_REFERENCE.to_string(),
                output_path: None,
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_export_subcommand_with_positional_output_path() {
        // given
        let args = vec!["export".to_string(), "conversation.md".to_string()];

        // when
        let parsed = parse_args(&args).expect("export with path should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: LATEST_SESSION_REFERENCE.to_string(),
                output_path: Some(PathBuf::from("conversation.md")),
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_export_subcommand_with_session_and_output_flags() {
        // given
        let args = vec![
            "export".to_string(),
            "--session".to_string(),
            "session-alpha".to_string(),
            "--output".to_string(),
            "/tmp/share.md".to_string(),
        ];

        // when
        let parsed = parse_args(&args).expect("export flags should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: "session-alpha".to_string(),
                output_path: Some(PathBuf::from("/tmp/share.md")),
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_export_subcommand_with_inline_flag_values() {
        // given
        let args = vec![
            "export".to_string(),
            "--session=session-beta".to_string(),
            "--output=/tmp/beta.md".to_string(),
        ];

        // when
        let parsed = parse_args(&args).expect("export inline flags should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: "session-beta".to_string(),
                output_path: Some(PathBuf::from("/tmp/beta.md")),
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_export_subcommand_with_json_output_format() {
        // given
        let args = vec![
            "--output-format=json".to_string(),
            "export".to_string(),
            "/tmp/notes.md".to_string(),
        ];

        // when
        let parsed = parse_args(&args).expect("json export should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: LATEST_SESSION_REFERENCE.to_string(),
                output_path: Some(PathBuf::from("/tmp/notes.md")),
                output_format: CliOutputFormat::Json,
            }
        );
    }

    #[test]
    fn rejects_unknown_export_options_with_helpful_message() {
        // given
        let args = vec!["export".to_string(), "--bogus".to_string()];

        // when
        let error = parse_args(&args).expect_err("unknown export option should fail");

        // then
        assert!(error.contains("unknown export option: --bogus"));
    }

    #[test]
    fn rejects_export_with_extra_positional_after_path() {
        // given
        let args = vec![
            "export".to_string(),
            "first.md".to_string(),
            "second.md".to_string(),
        ];

        // when
        let error = parse_args(&args).expect_err("multiple positionals should fail");

        // then
        assert!(error.contains("unexpected export argument: second.md"));
    }

    #[test]
    fn parse_export_args_helper_defaults_to_latest_reference_and_no_output() {
        // given
        let args: Vec<String> = vec![];

        // when
        let parsed = parse_export_args(&args, CliOutputFormat::Text)
            .expect("empty export args should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: LATEST_SESSION_REFERENCE.to_string(),
                output_path: None,
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_json_output_for_mcp_and_skills_commands() {
        assert_eq!(
            parse_args(&["--output-format=json".to_string(), "mcp".to_string()])
                .expect("json mcp should parse"),
            CliAction::Mcp {
                args: None,
                output_format: CliOutputFormat::Json,
            }
        );
        assert_eq!(
            parse_args(&[
                "--output-format=json".to_string(),
                "/skills".to_string(),
                "help".to_string(),
            ])
            .expect("json /skills help should parse"),
            CliAction::Skills {
                args: Some("help".to_string()),
                output_format: CliOutputFormat::Json,
            }
        );
    }

    #[test]
    fn single_word_slash_command_names_return_guidance_instead_of_hitting_prompt_mode() {
        let error = parse_args(&["cost".to_string()]).expect_err("cost should return guidance");
        assert!(error.contains("slash command"));
        assert!(error.contains("/cost"));
    }

    #[test]
    fn multi_word_prompt_still_uses_shorthand_prompt_mode() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        // Input is ["help", "me", "debug"] so the joined prompt shorthand
        // must be "help me debug". A previous batch accidentally rewrote
        // the expected string to "$help overview" (copy-paste slip).
        assert_eq!(
            parse_args(&["help".to_string(), "me".to_string(), "debug".to_string()])
                .expect("prompt shorthand should still work"),
            CliAction::Prompt {
                prompt: "help me debug".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: crate::default_permission_mode(),
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn parses_direct_agents_mcp_and_skills_slash_commands() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        assert_eq!(
            parse_args(&["/agents".to_string()]).expect("/agents should parse"),
            CliAction::Agents {
                args: None,
                output_format: CliOutputFormat::Text
            }
        );
        assert_eq!(
            parse_args(&["/mcp".to_string(), "show".to_string(), "demo".to_string()])
                .expect("/mcp show demo should parse"),
            CliAction::Mcp {
                args: Some("show demo".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["/skills".to_string()]).expect("/skills should parse"),
            CliAction::Skills {
                args: None,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["/skill".to_string()]).expect("/skill should parse"),
            CliAction::Skills {
                args: None,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["/skills".to_string(), "help".to_string()])
                .expect("/skills help should parse"),
            CliAction::Skills {
                args: Some("help".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["/skill".to_string(), "list".to_string()])
                .expect("/skill list should parse"),
            CliAction::Skills {
                args: Some("list".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&[
                "/skills".to_string(),
                "help".to_string(),
                "overview".to_string()
            ])
            .expect("/skills help overview should invoke"),
            CliAction::Prompt {
                prompt: "$help overview".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: crate::default_permission_mode(),
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
        assert_eq!(
            parse_args(&[
                "/skills".to_string(),
                "install".to_string(),
                "./fixtures/help-skill".to_string(),
            ])
            .expect("/skills install should parse"),
            CliAction::Skills {
                args: Some("install ./fixtures/help-skill".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["/skills".to_string(), "/test".to_string()])
                .expect("/skills /test should normalize to a single skill prompt prefix"),
            CliAction::Prompt {
                prompt: "$test".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: crate::default_permission_mode(),
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
        let error = parse_args(&["/status".to_string()])
            .expect_err("/status should remain REPL-only when invoked directly");
        assert!(error.contains("interactive-only"));
        assert!(error.contains("ace --resume SESSION.jsonl /status"));
    }

    #[test]
    fn direct_slash_commands_surface_shared_validation_errors() {
        let compact_error = parse_args(&["/compact".to_string(), "now".to_string()])
            .expect_err("invalid /compact shape should be rejected");
        assert!(compact_error.contains("Unexpected arguments for /compact."));
        assert!(compact_error.contains("Usage            /compact"));

        let plugins_error = parse_args(&[
            "/plugins".to_string(),
            "list".to_string(),
            "extra".to_string(),
        ])
        .expect_err("invalid /plugins list shape should be rejected");
        assert!(plugins_error.contains("Usage: /plugin list"));
        assert!(plugins_error.contains("Aliases          /plugins, /marketplace"));
    }

    #[test]
    fn formats_unknown_slash_command_with_suggestions() {
        let report = format_unknown_slash_command_message("statsu");
        assert!(report.contains("unknown slash command: /statsu"));
        assert!(report.contains("Did you mean"));
        assert!(report.contains("Use /help"));
    }

    #[test]
    fn formats_namespaced_omc_slash_command_with_contract_guidance() {
        let report = format_unknown_slash_command_message("oh-my-claudecode:hud");
        assert!(report.contains("unknown slash command: /oh-my-claudecode:hud"));
        assert!(report.contains("Claude Code/OMC plugin command"));
        assert!(report.contains("plugin slash commands"));
        assert!(report.contains("statusline"));
        assert!(report.contains("session hooks"));
    }

    #[test]
    fn parses_resume_flag_with_slash_command() {
        let args = vec![
            "--resume".to_string(),
            "session.jsonl".to_string(),
            "/compact".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("session.jsonl"),
                commands: vec!["/compact".to_string()],
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_resume_flag_without_path_as_latest_session() {
        assert_eq!(
            parse_args(&["--resume".to_string()]).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("latest"),
                commands: vec![],
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["--resume".to_string(), "/status".to_string()])
                .expect("resume shortcut should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("latest"),
                commands: vec!["/status".to_string()],
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_resume_flag_with_multiple_slash_commands() {
        let args = vec![
            "--resume".to_string(),
            "session.jsonl".to_string(),
            "/status".to_string(),
            "/compact".to_string(),
            "/cost".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("session.jsonl"),
                commands: vec![
                    "/status".to_string(),
                    "/compact".to_string(),
                    "/cost".to_string(),
                ],
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn rejects_unknown_options_with_helpful_guidance() {
        let error = parse_args(&["--resum".to_string()]).expect_err("unknown option should fail");
        assert!(error.contains("unknown option: --resum"));
        assert!(error.contains("Did you mean --resume?"));
        assert!(error.contains("ace --help"));
    }

    #[test]
    fn parses_resume_flag_with_slash_command_arguments() {
        let args = vec![
            "--resume".to_string(),
            "session.jsonl".to_string(),
            "/export".to_string(),
            "notes.txt".to_string(),
            "/clear".to_string(),
            "--confirm".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("session.jsonl"),
                commands: vec![
                    "/export notes.txt".to_string(),
                    "/clear --confirm".to_string(),
                ],
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_resume_flag_with_absolute_export_path() {
        let args = vec![
            "--resume".to_string(),
            "session.jsonl".to_string(),
            "/export".to_string(),
            "/tmp/notes.txt".to_string(),
            "/status".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("session.jsonl"),
                commands: vec!["/export /tmp/notes.txt".to_string(), "/status".to_string()],
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn filtered_tool_specs_respect_allowlist() {
        let allowed = ["read_file", "grep_search"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let filtered = filter_tool_specs(&GlobalToolRegistry::builtin(), Some(&allowed));
        let names = filtered
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["read_file", "grep_search"]);
    }

    #[test]
    fn filtered_tool_specs_include_plugin_tools() {
        let filtered = filter_tool_specs(&registry_with_plugin_tool(), None);
        let names = filtered
            .into_iter()
            .map(|definition| definition.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"bash".to_string()));
        assert!(names.contains(&"plugin_echo".to_string()));
    }

    #[test]
    fn format_connected_line_renders_anthropic_provider_for_claude_model() {
        let model = "claude-sonnet-4-6";

        let line = format_connected_line(model);

        assert_eq!(line, "Connected: claude-sonnet-4-6 via anthropic");
    }

    #[test]
    fn format_connected_line_renders_xai_provider_for_grok_model() {
        let model = "grok-3";

        let line = format_connected_line(model);

        assert_eq!(line, "Connected: grok-3 via xai");
    }

    #[test]
    fn resolve_repl_model_returns_user_supplied_model_unchanged_when_explicit() {
        let user_model = "claude-sonnet-4-6".to_string();

        let resolved = resolve_repl_model(user_model);

        assert_eq!(resolved, "claude-sonnet-4-6");
    }

    #[test]
    fn resolve_repl_model_falls_back_to_anthropic_model_env_when_default() {
        let _guard = env_lock();
        let root = temp_dir();
        fs::create_dir_all(&root).expect("root dir");
        let config_home = root.join("config");
        fs::create_dir_all(&config_home).expect("config home dir");
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("ANTHROPIC_MODEL");
        std::env::set_var("ANTHROPIC_MODEL", "sonnet");

        let resolved = with_current_dir(&root, || resolve_repl_model(DEFAULT_MODEL.to_string()));

        assert_eq!(resolved, "claude-sonnet-4-6");

        std::env::remove_var("ANTHROPIC_MODEL");
        std::env::remove_var("CLAW_CONFIG_HOME");
        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn resolve_repl_model_returns_default_when_env_unset_and_no_config() {
        let _guard = env_lock();
        let root = temp_dir();
        fs::create_dir_all(&root).expect("root dir");
        let config_home = root.join("config");
        fs::create_dir_all(&config_home).expect("config home dir");
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("ANTHROPIC_MODEL");

        let resolved = with_current_dir(&root, || resolve_repl_model(DEFAULT_MODEL.to_string()));

        assert_eq!(resolved, DEFAULT_MODEL);

        std::env::remove_var("CLAW_CONFIG_HOME");
        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn normalizes_supported_permission_modes() {
        assert_eq!(normalize_permission_mode("read-only"), Some("read-only"));
        assert_eq!(
            normalize_permission_mode("workspace-write"),
            Some("workspace-write")
        );
        assert_eq!(
            normalize_permission_mode("danger-full-access"),
            Some("danger-full-access")
        );
        assert_eq!(normalize_permission_mode("unknown"), None);
    }

    #[test]
    fn unknown_slash_command_guidance_suggests_nearby_commands() {
        let message = format_unknown_slash_command("stats");
        assert!(message.contains("Unknown slash command: /stats"));
        assert!(message.contains("/status"));
        assert!(message.contains("/help"));
    }

    #[test]
    fn unknown_omc_slash_command_guidance_explains_runtime_gap() {
        let message = format_unknown_slash_command("oh-my-claudecode:hud");
        assert!(message.contains("Unknown slash command: /oh-my-claudecode:hud"));
        assert!(message.contains("Claude Code/OMC plugin command"));
        assert!(message.contains("does not yet load plugin slash commands"));
    }
}
