@echo off
setlocal enabledelayedexpansion

:: OnlyDOMs Windows Initialization & Setup Script
:: Autonomous build, workspace configuration, and AI agent client registration for Windows

title OnlyDOMs Setup

set "REPO_DIR=%~dp0"
set "MCP_SERVER_DIR=%REPO_DIR%mcp-server"
set "EXTENSION_DIR=%REPO_DIR%extension"
set "BINARY_PATH=%MCP_SERVER_DIR%\target\release\onlydoms.exe"
set "PORT=9222"
set "REGISTER_ALL=0"

:: Parse arguments
:parse_args
if "%~1"=="" goto after_args
if /i "%~1"=="-i" set "REGISTER_ALL=1"
if /i "%~1"=="--install" set "REGISTER_ALL=1"
if /i "%~1"=="--register-all" set "REGISTER_ALL=1"
if /i "%~1"=="/i" set "REGISTER_ALL=1"
if /i "%~1"=="-p" (
    set "PORT=%~2"
    shift
)
if /i "%~1"=="--port" (
    set "PORT=%~2"
    shift
)
if /i "%~1"=="-h" goto show_help
if /i "%~1"=="--help" goto show_help
shift
goto parse_args

:show_help
echo ==========================================================
echo    OnlyDOMs Windows Setup Script
echo ==========================================================
echo Usage:
echo   setup.bat [OPTIONS]
echo.
echo Options:
echo   -i, --install, --register-all   Auto-build and register OnlyDOMs with detected
echo                                   MCP clients (VS Code, Cursor, Windsurf, JetBrains,
echo                                   Claude Desktop, Claude Code CLI).
echo   -p, --port ^<PORT^>               Specify default port (default: 9222).
echo   -h, --help                      Show this help message.
exit /b 0

:after_args
echo ==========================================================
echo    OnlyDOMs — Browser Automation for AI Coding Agents
echo ==========================================================

:: 1. Check Rust / Cargo
echo [*] Checking Rust and Cargo prerequisites...
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    if exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
        set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
    )
)

where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo [!] Cargo not found in PATH. Attempting automatic non-interactive rustup install...
    curl.exe -sSf -o "%TEMP%\rustup-init.exe" https://win.rustup.rs/x86_64
    if not exist "%TEMP%\rustup-init.exe" (
        powershell -Command "Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile '$env:TEMP\rustup-init.exe'"
    )
    if exist "%TEMP%\rustup-init.exe" (
        "%TEMP%\rustup-init.exe" -y
        set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
    )
)

where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo [ERROR] Rust / Cargo could not be found or installed automatically.
    echo Please install Rust from https://rustup.rs and rerun setup.bat.
    exit /b 1
)

for /f "tokens=*" %%v in ('cargo --version') do echo [+] Detected %%v

:: 2. Validate Chrome Extension
echo.
echo [*] Validating Chrome Extension directory...
if not exist "%EXTENSION_DIR%\manifest.json" (
    echo [ERROR] Extension manifest not found at %EXTENSION_DIR%\manifest.json
    exit /b 1
)
echo [+] Extension manifest verified.

:: 3. Build Release Binary
echo.
echo [*] Building OnlyDOMs Rust MCP server (release mode)...
cargo build --release --manifest-path "%MCP_SERVER_DIR%\Cargo.toml"
if %errorlevel% neq 0 (
    echo [ERROR] Cargo build failed.
    exit /b %errorlevel%
)

if not exist "%BINARY_PATH%" (
    echo [ERROR] Expected binary not found at %BINARY_PATH%
    exit /b 1
)
echo [+] Built binary: %BINARY_PATH%

:: 4. Generate Workspace Configs
echo.
echo [*] Generating workspace MCP configurations (.cursor, .vscode, .idea, .windsurf)...

:: Convert Windows backslashes to forward slashes for JSON compatibility
set "ESCAPED_BIN=%BINARY_PATH:\=/%"

if not exist "%REPO_DIR%.cursor" mkdir "%REPO_DIR%.cursor"
(
echo {
echo   "mcpServers": {
echo     "onlydoms": {
echo       "command": "${workspaceFolder}/mcp-server/target/release/onlydoms.exe",
echo       "args": ["--port", "%PORT%"]
echo     }
echo   }
echo }
) > "%REPO_DIR%.cursor\mcp.json"
echo [+] Created: .cursor\mcp.json

if not exist "%REPO_DIR%.vscode" mkdir "%REPO_DIR%.vscode"
(
echo {
echo   "mcpServers": {
echo     "onlydoms": {
echo       "command": "${workspaceFolder}/mcp-server/target/release/onlydoms.exe",
echo       "args": ["--port", "%PORT%"]
echo     }
echo   }
echo }
) > "%REPO_DIR%.vscode\mcp.json"
echo [+] Created: .vscode\mcp.json

if not exist "%REPO_DIR%.idea" mkdir "%REPO_DIR%.idea"
(
echo {
echo   "mcpServers": {
echo     "onlydoms": {
echo       "command": "%ESCAPED_BIN%",
echo       "args": ["--port", "%PORT%"]
echo     }
echo   }
echo }
) > "%REPO_DIR%.idea\mcp.json"
echo [+] Created: .idea\mcp.json

if not exist "%REPO_DIR%.windsurf" mkdir "%REPO_DIR%.windsurf"
(
echo {
echo   "mcpServers": {
echo     "onlydoms": {
echo       "command": "%ESCAPED_BIN%",
echo       "args": ["--port", "%PORT%"]
echo     }
echo   }
echo }
) > "%REPO_DIR%.windsurf\mcp.json"
echo [+] Created: .windsurf\mcp.json

:: 5. Register with MCP Clients if requested
if "%REGISTER_ALL%"=="1" (
    echo.
    echo [*] Registering with detected AI agent clients...

    :: Try using Python to register in JSON configs if Python exists
    where python >nul 2>nul
    if %errorlevel% equ 0 (
        python -c "import json, os, sys; p = os.path.expandvars(r'%%APPDATA%%\Claude\claude_desktop_config.json'); os.makedirs(os.path.dirname(p), exist_ok=True); d = json.load(open(p, encoding='utf-8')) if os.path.exists(p) else {}; d.setdefault('mcpServers', {})['onlydoms'] = {'command': r'%BINARY_PATH%', 'args': ['--port', '%PORT%']}; d.get('mcpServers', {}).pop('chromapilot', None); json.dump(d, open(p, 'w', encoding='utf-8'), indent=2); print('[+] Registered in Claude Desktop')" 2>nul
        python -c "import json, os, sys; p = os.path.expandvars(r'%%USERPROFILE%\.codeium\windsurf\mcp_config.json'); os.makedirs(os.path.dirname(p), exist_ok=True); d = json.load(open(p, encoding='utf-8')) if os.path.exists(p) else {}; d.setdefault('mcpServers', {})['onlydoms'] = {'command': r'%BINARY_PATH%', 'args': ['--port', '%PORT%']}; d.get('mcpServers', {}).pop('chromapilot', None); json.dump(d, open(p, 'w', encoding='utf-8'), indent=2); print('[+] Registered in Windsurf')" 2>nul
        python -c "import json, os, sys; p = os.path.expandvars(r'%%APPDATA%%\JetBrains\mcp.json'); os.makedirs(os.path.dirname(p), exist_ok=True); d = json.load(open(p, encoding='utf-8')) if os.path.exists(p) else {}; d.setdefault('mcpServers', {})['onlydoms'] = {'command': r'%BINARY_PATH%', 'args': ['--port', '%PORT%']}; d.get('mcpServers', {}).pop('chromapilot', None); json.dump(d, open(p, 'w', encoding='utf-8'), indent=2); print('[+] Registered in JetBrains / IntelliJ')" 2>nul
    )

    :: Claude Code CLI registration
    where claude >nul 2>nul
    if %errorlevel% equ 0 (
        claude mcp remove chromapilot >nul 2>nul
        claude mcp add onlydoms -- "%BINARY_PATH%" --port "%PORT%" >nul 2>nul
        echo [+] Registered with Claude Code CLI
    )
)

echo.
echo ==========================================================
echo    Initialization Complete! Next Steps:
echo ==========================================================
echo 1. Load Extension in Chrome:
echo    - Open chrome://extensions/
echo    - Enable 'Developer mode' (top right)
echo    - Click 'Load unpacked' and select:
echo      %EXTENSION_DIR%
echo.
echo 2. Run Daemon or MCP Server:
echo    %BINARY_PATH% --port %PORT%
echo.
echo 3. Click the OnlyDOMs icon in Chrome toolbar to connect.
echo.
exit /b 0
