# idx

Local semantic code search for your actual dependencies.

## What is idx?

When you ask an AI "how do I use library X?", it searches the web and gives you examples from blog posts, outdated docs, or random GitHub repos. Those examples might be for the wrong version, use deprecated APIs, or just not work with your setup.

**idx is different.** It reads your manifest files, downloads the exact packages and versions you're using, and indexes them locally. When you search, you're searching code that's actually in your dependency tree.

```bash
# You're using lodash 4.17.21 in your project
idx search "deep clone an object"

# Results come from lodash 4.17.21 - not Stack Overflow, not some random tutorial
```

### Why This Matters

- **Version-accurate**: Search the APIs that actually exist in your pinned versions
- **No hallucinations**: Results are real code from real packages you depend on
- **Works offline**: Everything is indexed locally after initial download
- **Private**: Only embedding requests hit the API—your code stays on your machine

## Quick Start

```bash
# Install
cargo install --path .

# Set your OpenAI API key (for embeddings)
idx config set-key sk-...

# Index all dependencies in your project
idx init

# Search
idx search "parse JSON from string"
```

That's it. idx scans your manifests, downloads sources, parses them with tree-sitter, generates embeddings, and stores everything in `.index/`.

## Commands

| Command | Description |
|---------|-------------|
| `idx init` | Scan manifests and index all dependencies |
| `idx update` | Re-index packages with changed versions |
| `idx watch` | Watch manifests and auto-reindex on changes |
| `idx index <pkg>` | Index a specific package (e.g., `npm:lodash@4.17.21`) |
| `idx search <query>` | Search indexed packages with natural language |
| `idx list` | List all indexed packages |
| `idx stats` | Show index statistics |
| `idx status` | Compare index vs manifest dependencies |
| `idx remove <pkg>` | Remove a package from the index |
| `idx prune` | Remove packages no longer in manifests |
| `idx clean` | Delete the entire `.index` directory |
| `idx mcp` | Run as MCP server (for AI tools) |
| `idx config` | Manage configuration |

## Supported Ecosystems

| Registry | Manifest | Notes |
|----------|----------|-------|
| npm | `package.json` | Uses package-lock.json for pinned versions |
| crates | `Cargo.toml` | Uses Cargo.lock for pinned versions |
| pypi | `pyproject.toml` | PEP 621 and Poetry formats |
| maven | `pom.xml` | Resolves `${property}` references |
| go | `go.mod` | Skips indirect dependencies |

## MCP Integration

idx runs as an [MCP](https://modelcontextprotocol.io) server, so AI tools like Claude Code can search your dependencies directly.

### Setup

Add to your Claude Code config (`~/.claude.json` or project `.mcp.json`):

```json
{
  "mcpServers": {
    "index": {
      "command": "idx",
      "args": ["mcp"]
    }
  }
}
```

### What This Enables

Once configured, Claude Code can:

- **Search your dependencies**: "How does reqwest handle timeouts?" → searches your actual reqwest version
- **Find real examples**: "Show me how to use serde_json::Value" → returns code from your indexed packages
- **Index on demand**: "Index the tokio crate" → adds it to your local index

No more hallucinated APIs. No more wrong-version examples. Just real code from your real dependencies.

## Configuration

Config lives at `~/.config/idx/config.toml`.

### API Key (Required)

idx uses OpenAI's embedding API:

```bash
idx config set-key sk-your-openai-key
```

### Using OpenRouter

Prefer [OpenRouter](https://openrouter.ai)?

```bash
idx config set-url https://openrouter.ai/api
idx config set-key sk-or-your-openrouter-key
idx config set-model openai/text-embedding-3-small
```

### View Config

```bash
idx config show
```

## How It Works

1. **Parse manifests** — Reads package.json, Cargo.toml, go.mod, etc.
2. **Download sources** — Fetches from npm, crates.io, PyPI, Maven Central, Go proxy
3. **Parse code** — Uses tree-sitter to extract functions, classes, types, docs
4. **Generate embeddings** — Creates vectors via your configured embedding API
5. **Store locally** — Saves to `.index/` using SQLite + LanceDB

```
.index/
├── index.db      # SQLite: package metadata, chunk info
├── vectors/      # LanceDB: vector embeddings
└── blobs/        # Content-addressed source storage
```

## Installation

```bash
# From source
cargo install --path .

# Or via curl
curl -fsSL https://raw.githubusercontent.com/noetic-sys/index/main/install.sh | sh

# Or via Homebrew
brew install noetic-sys/tap/idx
```

## License

AGPL-3.0-or-later
