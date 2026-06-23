mod client;
mod config;
mod deepsearch;
mod history;
mod provider;
mod search;

use std::io::{IsTerminal, Write};

use anyhow::Result;
use crossterm::style::Stylize;
use crossterm::{cursor, execute, terminal};
use indicatif::{ProgressBar, ProgressStyle};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use serde_json::{json, Value};
use termimad::MadSkin;
use unicode_width::UnicodeWidthChar;

use client::ChatClient;
use config::{load_env, load_last_model, save_last_model, tavily_api_key};
use deepsearch::deep_search_current_events;
use history::{list_conversations, load_conversation, save_conversation};
use provider::{api_key_for, current_provider, mask_key, set_api_key, set_provider, Provider};
use search::tavily_search_results;

const LOGO: &str = r"
 _                           _           _
| |_ ___ _ __ _ __ ___   ___| |__   __ _| |_
| __/ _ \ '__| '_ ` _ \ / __| '_ \ / _` | __|
| ||  __/ |  | | | | | | (__| | | | (_| | |_
 \__\___|_|  |_| |_| |_|\___|_| |_|\__,_|\__|";

fn term_size() -> (u16, u16) {
    terminal::size().unwrap_or((80, 24))
}

fn print_welcome(model: &str, provider: &str) {
    println!("{}", LOGO.cyan());
    println!(
        "  {} {}  {}  {} {}  {}  {} {} {}\n",
        "Provider:".dim(),
        provider.cyan(),
        "|".dim(),
        "Model:".dim(),
        model.cyan(),
        "|".dim(),
        "Type".dim(),
        "/help".bold(),
        "for commands".dim(),
    );
}

fn print_status(model: &str, client: &ChatClient, active_tool: Option<&str>) {
    let (prompt_tokens, completion_tokens) = client
        .last_usage
        .map(|u| (u.prompt_tokens, u.completion_tokens))
        .unwrap_or((0, 0));

    println!("  {} {}", "Model:".dim(), model.cyan());
    println!(
        "  {} {} in · {} out",
        "Tokens:".dim(),
        prompt_tokens,
        completion_tokens
    );
    if let Some(tool) = active_tool {
        println!("  {} {}", "Active Tool:".dim(), tool.yellow());
    }
}

fn print_help() {
    let rows = [
        ("/provider [name]", "Switch model provider & set its API key"),
        ("/model <name>", "Switch to a different model"),
        ("/models", "List popular models for the current provider"),
        ("/search <query>", "Manual web search via Tavily"),
        ("/deepsearch <topic>", "Run multi-step current-events research"),
        ("/save", "Save current conversation"),
        ("/load", "Load a previous conversation"),
        ("/history", "List saved conversations"),
        ("/clear", "Clear current conversation"),
        ("/help", "Show this help message"),
        ("/quit, /exit", "Exit termchat"),
    ];
    println!("\n  {}", "Commands".bold());
    for (cmd, desc) in rows {
        println!("    {:<22} {}", cmd.cyan(), desc.dim());
    }
    println!();
}

/// Natural-language shortcut: returns an equivalent `/command` if one matches.
fn handle_nl_shortcuts(cmd: &str, models: &[&str]) -> Option<String> {
    let lower = cmd.to_lowercase();
    if lower.contains("switch to model") || lower.contains("change model to") {
        let name = lower
            .replace("switch to model", "")
            .replace("change model to", "")
            .trim()
            .to_string();
        if models.iter().any(|m| m.to_lowercase() == name) {
            return Some(format!("/model {name}"));
        }
    }
    None
}

/// Arrow-key selector rendered in raw mode. Returns the chosen option or None.
fn select_from_list(options: &[&str], current: &str) -> Option<String> {
    if options.is_empty() || !std::io::stdin().is_terminal() {
        return None;
    }

    let n = options.len();
    let mut idx = options.iter().position(|o| *o == current).unwrap_or(0);
    let mut out = std::io::stdout();

    let render = |out: &mut std::io::Stdout, selected: usize| {
        for (i, opt) in options.iter().enumerate() {
            execute!(out, terminal::Clear(terminal::ClearType::CurrentLine)).ok();
            if i == selected {
                write!(out, "\x1b[1;36m  ❯ {opt}\x1b[0m").ok();
            } else {
                write!(out, "\x1b[2m    {opt}\x1b[0m").ok();
            }
            if i < n - 1 {
                write!(out, "\r\n").ok();
            }
        }
        out.flush().ok();
    };

    println!();
    let _ = terminal::enable_raw_mode();
    render(&mut out, idx);

    let mut result = None;
    loop {
        match crossterm::event::read() {
            Ok(crossterm::event::Event::Key(key)) => {
                use crossterm::event::{KeyCode, KeyModifiers};
                match key.code {
                    KeyCode::Up => idx = (idx + n - 1) % n,
                    KeyCode::Down => idx = (idx + 1) % n,
                    KeyCode::Enter => {
                        result = Some(options[idx].to_string());
                        break;
                    }
                    KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    _ => {}
                }
                // Return to the top of the list and redraw.
                if n > 1 {
                    execute!(out, cursor::MoveToPreviousLine(n as u16 - 1)).ok();
                }
                execute!(out, cursor::MoveToColumn(0)).ok();
                render(&mut out, idx);
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    // Clear the rendered list.
    if n > 1 {
        execute!(out, cursor::MoveToPreviousLine(n as u16 - 1)).ok();
    }
    execute!(
        out,
        cursor::MoveToColumn(0),
        terminal::Clear(terminal::ClearType::FromCursorDown)
    )
    .ok();
    let _ = terminal::disable_raw_mode();
    result
}

/// Stream a response, printing tokens live, then re-render the final message
/// as styled markdown in place.
async fn stream_response(
    client: &mut ChatClient,
    messages: &[Value],
    model: &str,
    skin: &MadSkin,
) -> Option<String> {
    let (width, height) = term_size();
    println!("{}", "─".repeat(width as usize).dim());

    let is_tty = std::io::stdout().is_terminal();
    let mut streamed = String::new();
    let mut rows: u16 = 0; // physical lines the streamed text occupies below the start
    let mut col: u16 = 0;

    let result = client
        .stream_chat(messages, model, |tok| {
            streamed.push_str(tok);
            print!("{tok}");
            std::io::stdout().flush().ok();
            for ch in tok.chars() {
                if ch == '\n' {
                    rows += 1;
                    col = 0;
                } else {
                    let w = ch.width().unwrap_or(0) as u16;
                    if col + w > width {
                        rows += 1;
                        col = w;
                    } else {
                        col += w;
                    }
                }
            }
        })
        .await;

    let content = match result {
        Ok(content) => content,
        Err(e) => {
            println!("\n{} {e}", "Error:".red());
            return None;
        }
    };

    // Re-render the final content as markdown, erasing the raw stream when it
    // still fits on screen. If it scrolled, leave the raw text as-is.
    if let Some(md) = content.as_ref() {
        if is_tty && rows < height.saturating_sub(1) {
            let mut out = std::io::stdout();
            execute!(out, cursor::MoveToColumn(0)).ok();
            if rows > 0 {
                execute!(out, cursor::MoveUp(rows)).ok();
            }
            execute!(out, terminal::Clear(terminal::ClearType::FromCursorDown)).ok();
            skin.print_text(md);
        } else if !is_tty {
            // raw already printed; nothing more to do
        }
        // (scrolled tty case keeps the raw stream)
    }

    println!();
    print_status(model, client, None);
    content.map(|c| c.trim().to_string()).filter(|c| !c.is_empty())
}

fn spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// Handle a slash command. Returns `false` to exit the REPL.
#[allow(clippy::too_many_arguments)]
async fn handle_command(
    cmd: &str,
    messages: &mut Vec<Value>,
    model: &mut String,
    provider: &mut Provider,
    client: &mut ChatClient,
    skin: &MadSkin,
) -> bool {
    let cmd = handle_nl_shortcuts(cmd, provider.popular_models()).unwrap_or_else(|| cmd.to_string());
    let mut parts = cmd.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or("").to_lowercase();
    let arg = parts.next().unwrap_or("").trim().to_string();

    match command.as_str() {
        "/quit" | "/exit" => {
            println!("  {}", "Goodbye!".dim());
            return false;
        }

        "/help" => print_help(),

        "/clear" => {
            messages.clear();
            println!("  {}", "Conversation cleared.".dim());
        }

        "/provider" => {
            let names: Vec<&str> = Provider::ALL.iter().map(|p| p.display_name()).collect();
            let chosen = if arg.is_empty() {
                select_from_list(&names, provider.display_name())
            } else {
                Provider::from_loose(&arg).map(|p| p.display_name().to_string())
            };

            match chosen.as_deref().and_then(Provider::from_loose) {
                None => {
                    if !arg.is_empty() {
                        println!(
                            "  {}",
                            format!("Unknown provider: {arg}. Options: cerebras, openrouter, fireworks.")
                                .dim()
                        );
                    }
                }
                Some(selected) => {
                    *provider = selected;
                    set_provider(selected);

                    let existing = api_key_for(selected);
                    let hint = if existing.is_empty() {
                        "none set".to_string()
                    } else {
                        mask_key(&existing)
                    };
                    print!(
                        "  {} ",
                        format!(
                            "Paste API key for {} [current: {hint}] (blank to keep): ",
                            selected.display_name()
                        )
                        .dim()
                    );
                    std::io::stdout().flush().ok();
                    let mut key = String::new();
                    std::io::stdin().read_line(&mut key).ok();
                    let key = key.trim();
                    if !key.is_empty() {
                        set_api_key(selected, key);
                    }

                    let api_key = api_key_for(selected);
                    client.set_provider(selected.base_url().to_string(), api_key.clone());

                    // Models differ per provider; reset to this provider's default.
                    *model = selected.default_model().to_string();
                    save_last_model(model);

                    println!(
                        "  {} {} {}",
                        "Switched to provider:".dim(),
                        selected.display_name().cyan(),
                        format!("(model: {model})").dim()
                    );
                    if api_key.is_empty() {
                        println!(
                            "  {}",
                            "No API key set — chat will fail until you run /provider again."
                                .yellow()
                        );
                    }
                }
            }
        }

        "/model" => {
            if arg.is_empty() {
                if let Some(chosen) = select_from_list(provider.popular_models(), model) {
                    *model = chosen;
                    save_last_model(model);
                    println!("  {} {}", "Switched to model:".dim(), model.clone().cyan());
                }
            } else {
                *model = arg;
                save_last_model(model);
                println!("  {} {}", "Switched to model:".dim(), model.clone().cyan());
            }
        }

        "/models" => {
            println!(
                "\n  {} {}",
                "Popular Models".bold(),
                format!("({})", provider.display_name()).dim()
            );
            for m in provider.popular_models() {
                let marker = if *m == model { " ✓" } else { "" };
                println!("    {}{}", m.cyan(), marker.dim());
            }
            println!();
        }

        "/search" => {
            if arg.is_empty() {
                println!("  {}", "Usage: /search <query>".dim());
            } else {
                let pb = spinner("Searching...");
                let results = tavily_search_results(&arg, 5).await;
                pb.finish_and_clear();
                match results {
                    Err(msg) => println!("{} {msg}", "Error:".red()),
                    Ok(results) if results.is_empty() => {
                        println!("  {}", "No results found.".dim())
                    }
                    Ok(results) => {
                        println!("\n  {} {}", "Search:".bold(), arg.clone().cyan());
                        for r in &results {
                            let title = r.get("title").and_then(Value::as_str).unwrap_or("No title");
                            let content =
                                r.get("content").and_then(Value::as_str).unwrap_or("");
                            let snippet: String = content.chars().take(150).collect();
                            let url = r.get("url").and_then(Value::as_str).unwrap_or("No url");
                            println!("  {}", title.bold().cyan());
                            println!("    {}", snippet.dim());
                            println!("    {}\n", url.blue().underlined());
                        }
                    }
                }
            }
        }

        "/deepsearch" => {
            if arg.is_empty() {
                println!("  {}", "Usage: /deepsearch <topic>".dim());
            } else {
                let stages = std::cell::RefCell::new(Vec::<String>::new());
                let pb = spinner("Deep search: starting...");
                let (report, metadata) = {
                    let progress = |stage: &str| {
                        stages.borrow_mut().push(stage.to_string());
                        pb.set_message(format!("Deep search: {}...", stage.to_lowercase()));
                    };
                    deep_search_current_events(&arg, client, model, &progress).await
                };
                pb.finish_and_clear();

                println!("{}", format!("── Deep Search: {arg} ──").dim());
                skin.print_text(&report);
                if let Some(errors) = metadata.get("errors").and_then(Value::as_array) {
                    if !errors.is_empty() {
                        println!("\n{}", "Warnings:".yellow());
                        for err in errors {
                            if let Some(s) = err.as_str() {
                                println!("  {}", format!("- {s}").dim());
                            }
                        }
                    }
                }

                messages.push(json!({"role": "user", "content": format!("/deepsearch {arg}")}));
                let mut meta = metadata;
                meta["type"] = json!("deepsearch");
                meta["stages"] = json!(stages.into_inner());
                messages.push(json!({
                    "role": "assistant",
                    "content": report,
                    "metadata": meta,
                }));
            }
        }

        "/save" => {
            if messages.is_empty() {
                println!("  {}", "No conversation to save.".dim());
            } else {
                match save_conversation(messages, model) {
                    Ok(path) => println!(
                        "  {} {}",
                        "Saved to:".dim(),
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    Err(e) => println!("{} {e}", "Error:".red()),
                }
            }
        }

        "/load" => {
            let files = list_conversations();
            if files.is_empty() {
                println!("  {}", "No saved conversations found.".dim());
            } else {
                for (i, f) in files.iter().take(20).enumerate() {
                    let stem = f.file_stem().unwrap_or_default().to_string_lossy();
                    println!("  {} {}", format!("{}", i + 1).bold(), stem);
                }
                print!("  {} ", "Load which #? (0 to cancel):".dim());
                std::io::stdout().flush().ok();
                let mut choice = String::new();
                std::io::stdin().read_line(&mut choice).ok();
                match choice.trim().parse::<usize>() {
                    Ok(0) => {}
                    Ok(num) if num >= 1 && num <= files.len() => {
                        match load_conversation(&files[num - 1]) {
                            Ok((loaded, loaded_model)) => {
                                *messages = loaded;
                                *model = loaded_model;
                                save_last_model(model);
                                println!(
                                    "  {}",
                                    format!(
                                        "Loaded {} ({} messages, model: {})",
                                        files[num - 1].file_name().unwrap_or_default().to_string_lossy(),
                                        messages.len(),
                                        model
                                    )
                                    .dim()
                                );
                            }
                            Err(e) => println!("{} {e}", "Error:".red()),
                        }
                    }
                    Ok(_) => println!("  {}", "Invalid selection.".dim()),
                    Err(_) => println!("  {}", "Invalid input.".dim()),
                }
            }
        }

        "/history" => {
            let files = list_conversations();
            if files.is_empty() {
                println!("  {}", "No saved conversations.".dim());
            } else {
                for f in files.iter().take(20) {
                    let stem = f.file_stem().unwrap_or_default().to_string_lossy();
                    println!("  {}", stem.dim());
                }
            }
        }

        other => {
            println!(
                "  {}",
                format!("Unknown command: {other}. Type /help for help.").dim()
            );
        }
    }

    true
}

#[tokio::main]
async fn main() -> Result<()> {
    load_env();

    let mut provider = current_provider();
    let api_key = api_key_for(provider);
    if api_key.is_empty() {
        eprintln!(
            "{} No API key set for {}. Run {} to choose a provider and paste its key.",
            "Warning:".yellow(),
            provider.display_name(),
            "/provider".bold()
        );
    }
    if tavily_api_key().is_empty() {
        eprintln!(
            "{} TAVILY_API_KEY not set; web search and /deepsearch will be unavailable.",
            "Warning:".yellow()
        );
    }

    let mut model = load_last_model(provider.default_model());
    let mut messages: Vec<Value> = Vec::new();
    let mut client = ChatClient::new(provider.base_url().to_string(), api_key)?;
    let skin = MadSkin::default();

    print_welcome(&model, provider.display_name());
    print_status(&model, &client, None);

    let mut rl = DefaultEditor::new()?;

    loop {
        let line = rl.readline(&format!("{} ", "›".green().bold()));
        let user_input = match line {
            Ok(l) => l.trim().to_string(),
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!("\n  {}", "Goodbye!".dim());
                break;
            }
            Err(_) => break,
        };

        if user_input.is_empty() {
            continue;
        }
        rl.add_history_entry(&user_input).ok();

        if user_input.starts_with('/')
            || handle_nl_shortcuts(&user_input, provider.popular_models()).is_some()
        {
            let should_continue = handle_command(
                &user_input,
                &mut messages,
                &mut model,
                &mut provider,
                &mut client,
                &skin,
            )
            .await;
            if !should_continue {
                break;
            }
            print_status(&model, &client, None);
            continue;
        }

        messages.push(json!({"role": "user", "content": user_input}));
        print_status(&model, &client, Some("chatting"));

        if let Some(content) = stream_response(&mut client, &messages, &model, &skin).await {
            messages.push(json!({"role": "assistant", "content": content}));
        }

        print_status(&model, &client, None);
    }

    Ok(())
}
