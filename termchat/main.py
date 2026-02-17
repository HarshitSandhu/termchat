import sys

from rich.console import Console
from rich.live import Live
from rich.markdown import Markdown
from rich.rule import Rule
from rich.spinner import Spinner
from rich.table import Table
from rich.text import Text
from rich.theme import Theme

from termchat.client import ChatClient
from termchat.config import OPENROUTER_API_KEY, POPULAR_MODELS, load_last_model, save_last_model
from termchat.history import list_conversations, load_conversation, save_conversation
from termchat.search import tavily_search

theme = Theme({"info": "dim", "success": "green", "warning": "yellow", "error": "red"})
console = Console(theme=theme)


def print_help():
    table = Table(title="Commands", show_header=False, box=None, padding=(0, 2))
    table.add_column(style="bold cyan")
    table.add_column(style="dim")
    table.add_row("/model <name>", "Switch to a different model")
    table.add_row("/models", "List popular OpenRouter models")
    table.add_row("/search <query>", "Manual web search via Tavily")
    table.add_row("/save", "Save current conversation")
    table.add_row("/load", "Load a previous conversation")
    table.add_row("/history", "List saved conversations")
    table.add_row("/clear", "Clear current conversation")
    table.add_row("/help", "Show this help message")
    table.add_row("/quit, /exit", "Exit termchat")
    console.print(table)


LOGO = r"""
 [dim cyan] _                           _           _
 | |_ ___ _ __ _ __ ___   ___| |__   __ _| |_
 | __/ _ \ '__| '_ ` _ \ / __| '_ \ / _` | __|
 | ||  __/ |  | | | | | | (__| | | | (_| | |_
  \__\___|_|  |_| |_| |_|\___|_| |_|\__,_|\__|[/dim cyan]
"""


def print_welcome(model: str):
    console.print(LOGO)
    console.print(
        f"  [dim]Model:[/dim] [cyan]{model}[/cyan]  [dim]|[/dim]  [dim]Type[/dim] [bold]/help[/bold] [dim]for commands[/dim]\n"
    )


def handle_command(
    cmd: str, messages: list[dict], model: str, client: ChatClient
) -> tuple[list[dict], str, bool]:
    """Handle a slash command. Returns (messages, model, should_continue)."""
    parts = cmd.split(maxsplit=1)
    command = parts[0].lower()
    arg = parts[1] if len(parts) > 1 else ""

    if command in ("/quit", "/exit"):
        console.print("  [dim]Goodbye![/dim]")
        return messages, model, False

    elif command == "/help":
        print_help()

    elif command == "/clear":
        messages.clear()
        console.print("  [dim]Conversation cleared.[/dim]")

    elif command == "/model":
        if not arg:
            console.print(f"  [dim]Current model:[/dim] [cyan]{model}[/cyan]")
            console.print("  [dim]Usage: /model <model-name>[/dim]")
        else:
            model = arg.strip()
            save_last_model(model)
            console.print(f"  [dim]Switched to model:[/dim] [cyan]{model}[/cyan]")

    elif command == "/models":
        table = Table(title="Popular Models", show_header=False, box=None)
        table.add_column(style="cyan")
        for m in POPULAR_MODELS:
            table.add_row(m)
        console.print(table)

    elif command == "/search":
        if not arg:
            console.print("  [dim]Usage: /search <query>[/dim]")
        else:
            with console.status("Searching..."):
                result = tavily_search(arg)
            console.print(Markdown(result))

    elif command == "/save":
        if not messages:
            console.print("  [dim]No conversation to save.[/dim]")
        else:
            path = save_conversation(messages, model)
            console.print(f"  [dim]Saved to:[/dim] {path.name}")

    elif command == "/load":
        files = list_conversations()
        if not files:
            console.print("  [dim]No saved conversations found.[/dim]")
        else:
            table = Table(show_header=True, box=None)
            table.add_column("#", style="bold")
            table.add_column("File")
            for i, f in enumerate(files[:20], 1):
                table.add_row(str(i), f.stem)
            console.print(table)
            choice = console.input("  [dim]Load which #? (0 to cancel):[/dim] ").strip()
            try:
                idx = int(choice) - 1
                if 0 <= idx < len(files):
                    messages_loaded, model_loaded = load_conversation(files[idx])
                    messages.clear()
                    messages.extend(messages_loaded)
                    model = model_loaded
                    save_last_model(model)
                    console.print(
                        f"  [dim]Loaded {files[idx].name} ({len(messages)} messages, model: {model})[/dim]"
                    )
                elif choice != "0":
                    console.print("  [dim]Invalid selection.[/dim]")
            except ValueError:
                console.print("  [dim]Invalid input.[/dim]")

    elif command == "/history":
        files = list_conversations()
        if not files:
            console.print("  [dim]No saved conversations.[/dim]")
        else:
            for f in files[:20]:
                console.print(f"  [dim]{f.stem}[/dim]")

    else:
        console.print(f"  [dim]Unknown command: {command}. Type /help for help.[/dim]")

    return messages, model, True


def stream_response(client: ChatClient, messages: list[dict], model: str) -> str | None:
    """Stream a response from the API with live markdown rendering. Returns full content."""
    gen = client.stream_chat(messages, model)
    full_content = ""

    console.print(Rule(style="dim"))

    try:
        with Live(
            Spinner("dots", text="Thinking...", style="dim"),
            console=console,
            refresh_per_second=12,
        ) as live:
            for token in gen:
                full_content += token
                try:
                    live.update(Markdown(full_content, code_theme="monokai"))
                except (IndexError, KeyError):
                    live.update(Text(full_content))
        console.print()

        # Display token usage and cost
        stats = client.get_generation_stats()
        if stats:
            cost = stats["cost"]
            cost_str = f"${cost:.4f}" if cost else "$0.0000"
            console.print(
                Text(
                    f"tokens: {stats['prompt_tokens']} in · {stats['completion_tokens']} out  |  cost: {cost_str}",
                    style="dim",
                )
            )
            console.print()
    except KeyboardInterrupt:
        console.print("\n  [dim]Response interrupted.[/dim]\n")

    return full_content.strip() or None


def main():
    if not OPENROUTER_API_KEY:
        console.print(
            "[red]Error:[/red] OPENROUTER_API_KEY not set. "
            "Copy .env.example to .env and add your key."
        )
        sys.exit(1)

    model = load_last_model()
    messages: list[dict] = []
    client = ChatClient()

    print_welcome(model)

    try:
        while True:
            try:
                user_input = console.input("[bold green]>[/bold green] ").strip()
            except EOFError:
                break

            if not user_input:
                continue

            if user_input.startswith("/"):
                messages, model, should_continue = handle_command(
                    user_input, messages, model, client
                )
                if not should_continue:
                    break
                continue

            messages.append({"role": "user", "content": user_input})

            content = stream_response(client, messages, model)
            if content:
                messages.append({"role": "assistant", "content": content})

    except KeyboardInterrupt:
        console.print("\n  [dim]Goodbye![/dim]")
    finally:
        client.close()


if __name__ == "__main__":
    main()
