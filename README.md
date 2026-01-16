# idx

Local semantic search for your project's dependencies. Use it standalone or as an MCP server for Claude Code.

## Getting Started

### 1. Install

```bash
brew install noetic-sys/tap/idx

# or: curl -fsSL https://raw.githubusercontent.com/noetic-sys/index/main/install.sh | sh
# or: cargo install --path .
```

### 2. Set your API key

idx uses OpenAI embeddings by default:

```bash
idx config set-key sk-your-openai-key
```

### 3. Index your project

In any project with a lockfile:

```bash
cd your-project
idx init
```

This reads your `package.json`, `Cargo.toml`, `go.mod`, etc., downloads the source for each dependency, and builds a local index in `.index/`.

### 4. Search

```bash
idx search "parse JSON from string"
```

### 5. (Optional) Set up MCP for Claude Code

This is the main use case—let Claude search your actual dependencies instead of hallucinating.

Add to `.mcp.json` in your project (or `~/.claude.json` globally):

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

Now Claude Code can search your indexed packages directly. Ask it "how do I use serde_json::Value?" and it'll return real code from your actual dependency versions.

## Commands

| Command | Description |
|---------|-------------|
| `idx init` | Scan manifests and index all dependencies |
| `idx update` | Re-index packages with changed versions |
| `idx watch` | Watch manifests and auto-reindex on changes |
| `idx index <pkg>` | Index a specific package (e.g., `npm:lodash@4.17.21`) |
| `idx search <query>` | Search indexed packages |
| `idx list` | List all indexed packages |
| `idx stats` | Show index statistics |
| `idx status` | Compare index vs manifest dependencies |
| `idx remove <pkg>` | Remove a package from the index |
| `idx prune` | Remove packages no longer in manifests |
| `idx clean` | Delete the entire `.index` directory |
| `idx mcp` | Run as MCP server |
| `idx config` | Manage configuration |

## Supported Ecosystems

| Registry | Manifest |
|----------|----------|
| npm | `package.json` / `package-lock.json` |
| crates | `Cargo.toml` / `Cargo.lock` |
| pypi | `pyproject.toml` |
| maven | `pom.xml` |
| go | `go.mod` |

## Configuration

Config lives at `~/.config/idx/config.toml`.

```bash
idx config set-key <key>      # Set API key
idx config set-url <url>      # Set API base URL (for OpenRouter, etc.)
idx config set-model <model>  # Set embedding model
idx config show               # View current config
```

### Using OpenRouter

```bash
idx config set-url https://openrouter.ai/api
idx config set-key sk-or-your-key
idx config set-model openai/text-embedding-3-small
```

## License

AGPL-3.0-or-later
