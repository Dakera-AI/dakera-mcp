# Contributing to Dakera

Thank you for your interest in contributing to Dakera! This document provides guidelines and instructions for contributing.

## Getting Started

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/your-feature`)
3. Make your changes
4. Run tests and linting
5. Commit your changes (`git commit -m 'Add your feature'`)
6. Push to your branch (`git push origin feature/your-feature`)
7. Open a Pull Request

## Development Setup

**Prerequisites:** Rust 1.75+ with cargo

```bash
# Build
cargo build

# Run tests
cargo test

# Lint
cargo clippy

# Check formatting
cargo fmt --check
```

## Code Style

- Follow the existing code style and conventions
- Write clear, descriptive commit messages
- Add tests for new functionality
- Update documentation as needed

## Pull Request Guidelines

- Keep PRs focused on a single change
- Include a clear description of what changed and why
- Ensure all CI checks pass
- Link relevant issues

## Reporting Issues

- Use the GitHub issue templates for bug reports and feature requests
- Include reproduction steps for bugs
- Provide environment details (OS, runtime version, etc.)

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
