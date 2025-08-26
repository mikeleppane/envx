# Contributing to envx

Thank you for your interest in contributing to envx! We're excited to have you join our community.
This document provides guidelines and instructions for contributing to the project.

## 📋 Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [How to Contribute](#how-to-contribute)
- [Development Workflow](#development-workflow)
- [Code Style Guidelines](#code-style-guidelines)
- [Testing Guidelines](#testing-guidelines)
- [Documentation](#documentation)
- [Commit Guidelines](#commit-guidelines)
- [Pull Request Process](#pull-request-process)
- [Release Process](#release-process)
- [Community](#community)

## 📜 Code of Conduct

By participating in this project, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md).
We are committed to providing a welcoming and inclusive environment for all contributors.

## 🚀 Getting Started

### Prerequisites

- **Rust**: 1.85.0 or later (install via [rustup](https://rustup.rs/))
- **Git**: For version control
- **GitHub Account**: For submitting pull requests

### Fork and Clone

1. Fork the repository on GitHub
2. Clone your fork locally:

   ```bash
   git clone https://github.com/mikeleppane/envx.git
   cd envx
   ```

3. Add the upstream repository:

   ```bash
   git remote add upstream https://github.com/mikeleppane/envx.git
   ```

## 🛠️ Development Setup

### Building the Project

```bash
# Build all workspace members
cargo build

# Build in release mode
cargo build --release

# Build a specific crate
cargo build -p envx-core
```

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p envx-core

# Run tests with output displayed
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

Or you can also use [nextest](https://nexte.st/) for faster test execution:

```bash
cargo nextest run --workspace  
```

### Running the Application

```bash
# Run the CLI
cargo run -- list

# Run the TUI
cargo run -- tui

# Run with debug logging
RUST_LOG=debug cargo run -- tui
```

### Development Tools

```bash
# Format code
cargo fmt

# Run clippy linter
cargo clippy -- -D warnings

# Check for security vulnerabilities
cargo audit

# Generate documentation
cargo doc --open
```

## 💡 How to Contribute

### Types of Contributions

We welcome various types of contributions:

- **Bug Fixes**: Fix issues reported in GitHub Issues
- **Features**: Implement new features or enhance existing ones
- **Documentation**: Improve README, API docs, or code comments
- **Tests**: Add missing tests or improve test coverage
- **Performance**: Optimize code for better performance
- **Refactoring**: Improve code structure and maintainability

### Finding Issues to Work On

1. Check the [Issues](https://github.com/mikeleppane/envx/issues) page
2. Look for issues labeled:
   - `good first issue` - Great for newcomers
   - `help wanted` - We need help with these
   - `enhancement` - New features or improvements
   - `bug` - Something needs fixing

### Proposing New Features

1. Check if the feature has already been proposed
2. Open a new issue with the `feature request` template
3. Describe the feature, use cases, and implementation approach
4. Wait for feedback before starting implementation

## 🔄 Development Workflow

### Branch Naming Convention

- `feature/description` - New features
- `fix/description` - Bug fixes
- `docs/description` - Documentation updates
- `refactor/description` - Code refactoring
- `test/description` - Test additions or fixes

### Workflow Steps

1. **Create a branch**:

   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes**:
   - Write code following our style guidelines
   - Add tests for new functionality
   - Update documentation as needed

3. **Test your changes**:

   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt --check
   ```

4. **Commit your changes** (see commit guidelines below)

5. **Push to your fork**:

   ```bash
   git push origin feature/your-feature-name
   ```

6. **Create a Pull Request** on GitHub

## 📝 Code Style Guidelines

### Rust Style

We follow the standard Rust style guidelines:

- Use `cargo fmt` to format code
- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use meaningful variable and function names
- Add comments for complex logic
- Prefer explicit over implicit

### Code Organization

```rust
// Good: Organized imports
use std::collections::HashMap;
use std::fs;

use color_eyre::Result;
use serde::{Deserialize, Serialize};

use crate::error::EnvxError;

// Good: Clear function documentation
/// Loads environment variables from the specified source.
///
/// # Arguments
///
/// * `source` - The source to load variables from
///
/// # Returns
///
/// A Result containing a vector of EnvVar or an error
pub fn load_from_source(source: Source) -> Result<Vec<EnvVar>> {
    // Implementation
}
```

### Error Handling

- Use `Result<T, E>` for fallible operations
- Create custom error types when appropriate
- Provide helpful error messages
- Use `?` operator for error propagation

## 🧪 Testing Guidelines

### Test Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_description() {
        // Arrange
        let input = "test";
        
        // Act
        let result = function_under_test(input);
        
        // Assert
        assert_eq!(result, expected_value);
    }

    #[test]
    #[should_panic(expected = "error message")]
    fn test_error_condition() {
        // Test error handling
    }
}
```

### Testing Best Practices

- Write tests for all new functionality
- Aim for >80% code coverage
- Test edge cases and error conditions
- Use descriptive test names
- Keep tests focused and independent
- Use test fixtures for complex data

### Integration Tests

Place integration tests in the `tests/` directory:

```rust
// tests/integration_test.rs
use envx_core::EnvVarManager;

#[test]
fn test_end_to_end_workflow() {
    // Test complete workflows
}
```

## 📚 Documentation

### Code Documentation

- Add doc comments (`///`) for public APIs
- Include examples in doc comments
- Document panics, errors, and safety concerns
- Keep documentation up-to-date with code changes

```rust
/// Sets an environment variable with the specified name and value.
///
/// # Arguments
///
/// * `name` - The variable name
/// * `value` - The variable value
/// * `persistent` - Whether to persist across sessions
///
/// # Examples
///
/// ```
/// use envx_core::EnvVarManager;
///
/// let mut manager = EnvVarManager::new();
/// manager.set("MY_VAR", "value", false)?;
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The name contains invalid characters
/// - System permissions are insufficient
pub fn set(&mut self, name: String, value: String, persistent: bool) -> Result<()> {
    // Implementation
}
```

### README and User Documentation

- Update README.md for user-facing changes
- Add examples for new features
- Update command-line usage documentation
- Include screenshots for UI changes

## 📝 Commit Guidelines

We follow [Conventional Commits](https://www.conventionalcommits.org/):

### Commit Format

```text
<type>(<scope>): <subject>

<body>

<footer>
```

### Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, semicolons, etc.)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Adding or modifying tests
- `build`: Build system changes
- `ci`: CI/CD changes
- `chore`: Other changes (update dependencies, etc.)

### Examples

```bash
# Feature
git commit -m "feat(tui): add multi-line editing support"

# Bug fix
git commit -m "fix(core): handle empty variable names correctly"

# Documentation
git commit -m "docs: update CLI usage examples"

# With body
git commit -m "feat(export): add YAML export format

- Add serde_yaml dependency
- Implement YamlExporter
- Add tests for YAML serialization

Closes #123"
```

## 🔄 Pull Request Process

### Before Submitting

1. **Update from upstream**:

   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

2. **Run all checks**:

   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt --check
   cargo doc --no-deps
   ```

3. **Update documentation** if needed

### PR Guidelines

1. **Title**: Use a clear, descriptive title following commit conventions
2. **Description**: Fill out the PR template completely
3. **Link Issues**: Reference related issues (e.g., "Closes #123")
4. **Screenshots**: Include screenshots for UI changes
5. **Breaking Changes**: Clearly mark and document any breaking changes

### Review Process

1. At least one maintainer approval required
2. All CI checks must pass
3. No merge conflicts
4. Code coverage should not decrease significantly

### After Merge

- Delete your feature branch
- Update your local main branch
- Celebrate your contribution! 🎉

## 📦 Release Process

### Version Numbering

We use [Semantic Versioning](https://semver.org/):

- **Major**: Breaking changes
- **Minor**: New features, backward compatible
- **Patch**: Bug fixes, backward compatible

### Release Steps

1. Update version in `Cargo.toml` files
2. Update CHANGELOG.md
3. Create a release PR
4. After merge, tag the release
5. GitHub Actions will build and publish

## 👥 Community

### Getting Help

- **GitHub Issues**: For bug reports and feature requests
- **Discussions**: For questions and general discussion
- **Email**: <mleppan23@gmail.com> for security concerns

### Recognition

Contributors are recognized in:

- The project README
- Release notes
- Our [Contributors](https://github.com/mikeleppane/envx/graphs/contributors) page

## 🙏 Thank You

Your contributions make envx better for everyone. We appreciate your time and effort in improving the project.
Happy coding! 🚀

---

<p align="center">
  <strong>Questions?</strong> Feel free to open an issue or reach out to the maintainers.
</p>
