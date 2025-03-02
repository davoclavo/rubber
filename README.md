# 🦆 Rubber - Your AI-Powered PR Review Buddy

Rubber is a command-line tool that helps you review GitHub Pull Requests by providing automated analysis and AI-powered code review feedback. Think of it as having a helpful pair programming buddy available 24/7.

## 🚀 Features

- View recent PRs in a repository
- Detailed PR analysis including:
  - Line changes statistics
  - File-by-file diff review
  - Automated code pattern detection
  - AI-powered code review using Claude
- Interactive PR exploration
- Comment history viewing

## 📋 Requirements

- Rust installed on your system
- An Anthropic API key for AI-powered reviews
- GitHub access to the repositories you want to review

## ⚙️ Setup

1. Clone the repository
2. Set your Anthropic API key:
   ```bash
   export ANTHROPIC_API_KEY='your-key-here'
   ```
3. Configure logging level (optional):
   ```bash
   export RUST_LOG=info  # Options: error, warn, info, debug, trace
   ```
4. Build the project:
   ```bash
   cargo build --release
   ```

## 🎮 Usage

### Basic Usage

```bash
rubber <owner> <repo> [pr_number]
```

### Environment Variables

- `ANTHROPIC_API_KEY`: Your Anthropic API key for AI-powered reviews
- `RUST_LOG`: Logging level configuration (default: info)
  - Available levels: error, warn, info, debug, trace
  - Example: `RUST_LOG=debug` for verbose logging

### Examples

List recent PRs:
```bash
rubber microsoft typescript
```

Review specific PR with debug logging:
```bash
RUST_LOG=debug rubber microsoft typescript 12345
```

## 🔍 Current Analysis Features

- Line change statistics
- Detection of common code patterns:
  - TODO/FIXME comments
  - Debug statements (println!, dbg!)
  - Unwrap usage
  - Panic statements
  - Avoid magic constants
- AI-powered code review feedback
- Comment history tracking

## 🎯 Future Roadmap

### Enhanced Code Analysis

- **Pattern Matching**
  - Custom pattern definition support
  - Language-specific idiom checking
  - Anti-pattern detection
  - Code complexity metrics

- **Security Analysis**
  - Credential scanning
  - Dependency vulnerability checking
  - Permission changes detection
  - API security best practices

- **Style Enforcement**
  - Custom style rule configuration
  - Automatic formatting suggestions
  - Team convention compliance checking

- **Performance Impact**
  - Runtime complexity analysis
  - Memory usage estimation
  - Database query impact
  - API call overhead detection

- **Breaking Change Detection**
  - API signature changes
  - Database schema modifications
  - Configuration format updates
  - Dependency version conflicts

### Planned Features

- **CI/CD Integration**
  - Automated PR comments
  - Status checks integration
  - Review blocking on critical issues

- **Team Collaboration**
  - Review assignment automation
  - Knowledge sharing from past reviews
  - Team-specific rule sets

- **Interactive Features**
  - In-line code suggestions
  - Interactive fix application
  - Review checklist automation

- **Reporting**
  - PR quality metrics
  - Team review statistics
  - Common issue tracking
  - Review time analytics

## 🤝 Contributing

Contributions are welcome! Feel free to:

1. Fork the repository
2. Create a feature branch
3. Submit a Pull Request


## 🙏 Acknowledgments

- Powered by Anthropic's Claude for AI code review
- Built with Rust 🦀
- Inspired by the rubber duck debugging method

## 📞 Support

- Create an issue for bug reports
- Start a discussion for feature requests
- Check out our contributing guidelines

Remember: Rubber is meant to *assist* human reviewers, not replace them. Always verify AI suggestions and use your judgment!

## Features

### Linus Torvalds Mode

Want to get feedback on your PR in the style of Linux's creator? Use the `--linus-torvalds` flag to receive 
code review feedback in the characteristically passionate and direct style of Linus Torvalds.

```bash
rubber owner repo PR_number --linus-torvalds
```

This mode will:
- Provide brutally honest feedback about code quality
- Point out potential issues with extra... enthusiasm
- Channel Linus's famous attention to performance and maintainability
- Keep technical accuracy while adding some colorful commentary

Example output:
```
┣━━ Linus Says ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
┃
┃  Jesus Christ, people! Who taught you to handle errors like this? Your 
┃  unwrap() calls are scattered around like a drunk monkey was throwing 
┃  bananas at the keyboard! Have you never heard of proper error handling?
┃  
┃  And don't even get me started on those magic numbers. They make me want to 
┃  pull my hair out and go live in a cave where I never have to see code again.
┃  
┃  Look, it's not rocket science:
┃  1. USE PROPER ERROR TYPES
┃  2. DOCUMENT YOUR CONSTANTS
┃  3. STOP BEING LAZY WITH MEMORY MANAGEMENT
┃
```

Note: While entertaining, this mode still provides technically valid code review feedback, just with 
extra... personality.
