#!/usr/bin/env bash
# ChromaPilot Initialization & Setup Script
# Automatically builds the Rust MCP server, validates the Chrome extension,
# generates workspace MCP configs (.cursor, .vscode), and registers with AI agent clients.

set -e

# Terminal colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Resolve physical canonical repo directory (resolving any symlinks)
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
MCP_SERVER_DIR="$REPO_DIR/mcp-server"
EXTENSION_DIR="$REPO_DIR/extension"
BINARY_PATH="$MCP_SERVER_DIR/target/release/chromapilot"
PORT="9222"

# Flags
REGISTER_ALL=false
REGISTER_CLAUDE=false
REGISTER_WINDSURF=false
REGISTER_ANTIGRAVITY=false
AUTO_INSTALL_RUST=false
WRITE_WORKSPACE_CONFIG=true

show_help() {
    cat <<EOF
ChromaPilot Setup & Auto-Initialization Script

Usage:
  ./setup.sh [OPTIONS]

Options:
  -i, --install, --register-all   Auto-build and register ChromaPilot with all detected
                                  MCP clients (Claude Desktop, Windsurf, Antigravity, Claude Code CLI).
  --register-claude               Register with Claude Desktop config.
  --register-windsurf             Register with Windsurf MCP config.
  --register-antigravity          Register with Antigravity / Gemini CLI config.
  --auto-install-rust             Attempt to install Rust/Cargo via rustup if not present.
  --no-workspace-config           Skip generating project-level .cursor/mcp.json and .vscode/mcp.json.
  -p, --port <PORT>               Specify default port for MCP server (default: 9222).
  -h, --help                      Show this help message.

Examples:
  ./setup.sh                      Standard build + workspace config generation
  ./setup.sh --install            Full auto-initialization for AI coding agents
  ./setup.sh --port 9922          Build and configure for alternative port 9922
EOF
    exit 0
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            show_help
            ;;
        -i|--install|--register-all)
            REGISTER_ALL=true
            REGISTER_CLAUDE=true
            REGISTER_WINDSURF=true
            REGISTER_ANTIGRAVITY=true
            shift
            ;;
        --register-claude)
            REGISTER_CLAUDE=true
            shift
            ;;
        --register-windsurf)
            REGISTER_WINDSURF=true
            shift
            ;;
        --register-antigravity)
            REGISTER_ANTIGRAVITY=true
            shift
            ;;
        --auto-install-rust)
            AUTO_INSTALL_RUST=true
            shift
            ;;
        --no-workspace-config)
            WRITE_WORKSPACE_CONFIG=false
            shift
            ;;
        -p|--port)
            PORT="$2"
            shift 2
            ;;
        *)
            echo -e "${YELLOW}Unknown option: $1 (run ./setup.sh --help for options)${NC}"
            shift
            ;;
    esac
done

echo -e "${CYAN}${BOLD}"
echo "=========================================================="
echo "    ChromaPilot — Browser Control for AI Agents"
echo "=========================================================="
echo -e "${NC}"

# 1. Check prerequisites
echo -e "${BLUE}▶ Checking prerequisites...${NC}"

# Try sourcing cargo environment if not already in PATH
if ! command -v cargo &> /dev/null; then
    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    fi
fi

# Auto-install Rust if requested and missing
if ! command -v cargo &> /dev/null; then
    if [ "$AUTO_INSTALL_RUST" = true ]; then
        echo -e "  ${YELLOW}ℹ${NC} Rust/Cargo not found. Attempting non-interactive installation via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        if [ -f "$HOME/.cargo/env" ]; then
            # shellcheck source=/dev/null
            source "$HOME/.cargo/env"
        fi
    fi
fi

if ! command -v cargo &> /dev/null; then
    echo -e "${RED}✘ Rust and Cargo are not installed or not in PATH.${NC}"
    echo -e "  Please install Rust via: ${YELLOW}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}"
    echo -e "  Or re-run with: ${YELLOW}./setup.sh --auto-install-rust${NC}"
    exit 1
fi

RUST_VERSION=$(cargo --version)
echo -e "  ${GREEN}✔${NC} Rust/Cargo found: ${RUST_VERSION}"

# Check Chrome / Chromium
CHROME_FOUND=false
if [[ "$OSTYPE" == "darwin"* ]]; then
    if [[ -d "/Applications/Google Chrome.app" ]] || [[ -d "$HOME/Applications/Google Chrome.app" ]] || [[ -d "/Applications/Chromium.app" ]]; then
        CHROME_FOUND=true
    fi
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    if command -v google-chrome &> /dev/null || command -v chromium-browser &> /dev/null || command -v chromium &> /dev/null; then
        CHROME_FOUND=true
    fi
fi

if [ "$CHROME_FOUND" = true ]; then
    echo -e "  ${GREEN}✔${NC} Google Chrome / Chromium detected"
else
    echo -e "  ${YELLOW}ℹ${NC} Chrome not auto-detected in standard paths (ensure Google Chrome is installed)"
fi

# 2. Validate Chrome Extension directory
echo -e "\n${BLUE}▶ Validating Chrome Extension...${NC}"
if [[ -f "$EXTENSION_DIR/manifest.json" ]]; then
    echo -e "  ${GREEN}✔${NC} Extension directory verified: ${EXTENSION_DIR}"
else
    echo -e "${RED}✘ Extension manifest not found at: ${EXTENSION_DIR}/manifest.json${NC}"
    exit 1
fi

# 3. Build MCP Server in release mode
echo -e "\n${BLUE}▶ Building ChromaPilot Rust MCP Server (release mode)...${NC}"
cargo build --release --manifest-path "$MCP_SERVER_DIR/Cargo.toml"

if [[ ! -f "$BINARY_PATH" ]]; then
    echo -e "${RED}✘ Binary build failed: ${BINARY_PATH} not found.${NC}"
    exit 1
fi

echo -e "  ${GREEN}✔${NC} Built binary: ${BINARY_PATH}"

# 4. Workspace MCP Configs (.cursor/mcp.json and .vscode/mcp.json)
if [ "$WRITE_WORKSPACE_CONFIG" = true ]; then
    echo -e "\n${BLUE}▶ Writing project workspace MCP configurations...${NC}"
    
    mkdir -p "$REPO_DIR/.cursor"
    cat > "$REPO_DIR/.cursor/mcp.json" <<EOF
{
  "mcpServers": {
    "chromapilot": {
      "command": "\${workspaceFolder}/mcp-server/target/release/chromapilot",
      "args": ["--port", "${PORT}"]
    }
  }
}
EOF
    echo -e "  ${GREEN}✔${NC} Project config created: .cursor/mcp.json (Cursor IDE zero-config)"

    mkdir -p "$REPO_DIR/.vscode"
    cat > "$REPO_DIR/.vscode/mcp.json" <<EOF
{
  "mcpServers": {
    "chromapilot": {
      "command": "\${workspaceFolder}/mcp-server/target/release/chromapilot",
      "args": ["--port", "${PORT}"]
    }
  }
}
EOF
    echo -e "  ${GREEN}✔${NC} Project config created: .vscode/mcp.json (VS Code / Cline / Roo Code)"
fi

# 5. Client Auto-Registration (Claude Desktop, Windsurf, Antigravity, Claude Code CLI)
PYTHON_CMD=""
if command -v python3 &> /dev/null; then
    PYTHON_CMD="python3"
elif command -v python &> /dev/null; then
    PYTHON_CMD="python"
fi

register_mcp_config() {
    local client_name="$1"
    local config_file="$2"
    
    if [ -z "$PYTHON_CMD" ]; then
        echo -e "  ${YELLOW}ℹ${NC} Python not available; skipping auto-merge for ${client_name}."
        return
    fi

    "$PYTHON_CMD" - <<EOF
import json, os, sys

config_path = os.path.expanduser("$config_file")
binary_path = "$BINARY_PATH"
port = "$PORT"

try:
    os.makedirs(os.path.dirname(config_path), exist_ok=True)
    data = {}
    if os.path.exists(config_path):
        try:
            with open(config_path, "r", encoding="utf-8") as f:
                data = json.load(f)
        except Exception:
            data = {}

    if not isinstance(data, dict):
        data = {}

    if "mcpServers" not in data or not isinstance(data["mcpServers"], dict):
        data["mcpServers"] = {}

    data["mcpServers"]["chromapilot"] = {
        "command": binary_path,
        "args": ["--port", port]
    }

    with open(config_path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)

    print(f"  \033[0;32m✔\033[0m Successfully registered in {config_path}")
except Exception as e:
    print(f"  \033[1;33mℹ\033[0m Could not update {config_path}: {e}")
EOF
}

if [ "$REGISTER_CLAUDE" = true ]; then
    echo -e "\n${BLUE}▶ Registering with Claude Desktop...${NC}"
    CLAUDE_CONFIG=""
    if [[ "$OSTYPE" == "darwin"* ]]; then
        CLAUDE_CONFIG="$HOME/Library/Application Support/Claude/claude_desktop_config.json"
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        CLAUDE_CONFIG="$HOME/.config/Claude/claude_desktop_config.json"
    elif [[ "$OSTYPE" == "msys"* || "$OSTYPE" == "win32"* ]]; then
        CLAUDE_CONFIG="$APPDATA/Claude/claude_desktop_config.json"
    fi
    if [ -n "$CLAUDE_CONFIG" ]; then
        register_mcp_config "Claude Desktop" "$CLAUDE_CONFIG"
    fi
fi

if [ "$REGISTER_WINDSURF" = true ]; then
    echo -e "\n${BLUE}▶ Registering with Windsurf...${NC}"
    WINDSURF_CONFIG="$HOME/.codeium/windsurf/mcp_config.json"
    register_mcp_config "Windsurf" "$WINDSURF_CONFIG"
fi

if [ "$REGISTER_ANTIGRAVITY" = true ]; then
    echo -e "\n${BLUE}▶ Registering with Antigravity / Gemini CLI...${NC}"
    ANTIGRAVITY_CONFIG="$HOME/.gemini/antigravity/mcp_config.json"
    register_mcp_config "Antigravity" "$ANTIGRAVITY_CONFIG"
fi

# Claude Code CLI registration
if [ "$REGISTER_ALL" = true ] && command -v claude &> /dev/null; then
    echo -e "\n${BLUE}▶ Registering with Claude Code CLI...${NC}"
    if claude mcp add chromapilot -- "$BINARY_PATH" --port "$PORT" &> /dev/null; then
        echo -e "  ${GREEN}✔${NC} Registered with Claude Code CLI (claude mcp add)"
    else
        echo -e "  ${YELLOW}ℹ${NC} Note: claude mcp add exited with non-zero status or was already registered"
    fi
fi

# 6. Output Summary & Next Steps
echo -e "\n${GREEN}${BOLD}==========================================================${NC}"
echo -e "${GREEN}${BOLD}    Initialization Complete! Next Steps:${NC}"
echo -e "${GREEN}${BOLD}==========================================================${NC}"

echo -e "\n${BOLD}1. Load the Chrome Extension:${NC}"
echo -e "   • ${BOLD}Via Chrome UI${NC}:"
echo -e "     a. Open Chrome and navigate to ${CYAN}chrome://extensions/${NC}"
echo -e "     b. Toggle ${BOLD}Developer mode${NC} ON (upper right corner)."
echo -e "     c. Click ${BOLD}Load unpacked${NC} and choose: ${YELLOW}${EXTENSION_DIR}${NC}"
echo -e "   • ${BOLD}Or via Terminal one-liner${NC}:"
if [[ "$OSTYPE" == "darwin"* ]]; then
    echo -e "     ${CYAN}open -a \"Google Chrome\" --args --load-extension=\"${EXTENSION_DIR}\"${NC}"
else
    echo -e "     ${CYAN}google-chrome --load-extension=\"${EXTENSION_DIR}\"${NC}"
fi

echo -e "\n${BOLD}2. MCP Client Status:${NC}"
if [ "$REGISTER_ALL" = true ]; then
    echo -e "   ${GREEN}✔${NC} Auto-registered with detected MCP clients."
else
    echo -e "   Workspace configs created in ${CYAN}.cursor/mcp.json${NC} and ${CYAN}.vscode/mcp.json${NC}."
    echo -e "   To auto-register into Claude Desktop / Windsurf / Antigravity globally, run:"
    echo -e "     ${YELLOW}./setup.sh --install${NC}"
fi

echo -e "\n${BOLD}3. Ready-to-use MCP Configuration Snippet:${NC}"
cat <<EOF
{
  "mcpServers": {
    "chromapilot": {
      "command": "${BINARY_PATH}",
      "args": ["--port", "${PORT}"]
    }
  }
}
EOF

echo -e "\n${BOLD}4. Verify Connection:${NC}"
echo -e "   - When your agent launches ChromaPilot or when you start it directly:"
echo -e "     ${YELLOW}${BINARY_PATH} --port ${PORT}${NC}"
echo -e "   - Click the ChromaPilot icon in the Chrome toolbar."
echo -e "   - The badge will display 🟢 ${BOLD}Connected${NC} (Port ${PORT})."
echo -e "   - Done! Your agent now has full, token-efficient control of Google Chrome.\n"
