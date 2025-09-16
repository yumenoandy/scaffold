# Repository Guidelines

## Project Structure & Module Organization
- `src/`: Python package code (e.g., `src/python_cli/`).
- `src/python_cli/cli.py`: CLI entry point (or `__main__.py`).
- `tests/`: Pytest test suite (`tests/test_*.py`).
- `pyproject.toml`: Project config; dependencies, scripts, and entry-points.
- `scripts/` (optional): Developer utilities and one-off tools.

## Build, Test, and Development Commands (uv-only)
- Prereqs: `uv` installed.
- Create venv: `uv venv && source .venv/bin/activate`.
- Install (dev): `uv pip install -e '.[dev]'` (or `-e .` if no extras).
- Run CLI: `uv run -m python_cli --help`.
- Run tests: `uv run pytest -q`.
- Coverage: `uv run pytest --cov=python_cli -q`.
- Lint: `uv run ruff check .`.
- Format: `uv run ruff format .`.

## Coding Style & Naming Conventions
- 4-space indentation, 120-char soft line limit.
- Names: `snake_case` functions/vars, `PascalCase` classes, `UPPER_CASE` constants.
- Type hints for public APIs; prefer `from __future__ import annotations`.
- Docstrings on modules and public functions; one-line summaries.

## Testing Guidelines
- Framework: `pytest`; test files under `tests/` as `test_*.py`.
- Target meaningful unit coverage; mock network/IO (`pytest-mock`/`unittest.mock`).
- Add regression tests with each bug fix to lock behavior.

## Commit & Pull Request Guidelines
- Commits: Conventional Commits (e.g., `feat: add run subcommand`).
- Keep changes focused; imperative, present-tense messages.
- PRs: clear description, linked issue, `uv run pytest -q` output, and usage notes.
- Add screenshots or CLI examples when behavior changes.

## Security & Configuration Tips
- Use environment variables (e.g., `OPENAI_API_KEY`). Optionally load via `.env` in development.
- Never commit secrets; ignore `.env*`, `.coverage`, and cache/artefacts.
- Avoid logging sensitive data; redact request/response bodies.

## Agent/CLI Design Notes
- Prefer modern `typer` with type-hinted commands; place subcommands in `src/python_cli/commands/`.
- Use `Annotated[...]` with `typer.Option`/`typer.Argument`, `Enum`/`pathlib.Path`, and sensible defaults.
- Prefer `typer.echo` over `print`; use `typer.Exit`, `typer.BadParameter` for flow/validation.
- Avoid global state; inject configuration and clients.
- Add retries/timeouts for network calls; return clear, actionable errors.
