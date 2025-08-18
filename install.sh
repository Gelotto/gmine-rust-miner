#!/bin/sh

# install.sh - Installer for the GMINE Rust Miner
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Gelotto/gmine-rust-miner/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/Gelotto/gmine-rust-miner/main/install.sh | sh -s -- --from-source
#   curl -fsSL https://raw.githubusercontent.com/Gelotto/gmine-rust-miner/main/install.sh | sh -s -- --help
#
# Environment variables:
#   GMINE_INSTALL_DIR - Override installation directory (default: $HOME/.gmine)
#
# Based on the rustup installer pattern - https://rustup.rs

set -e # Exit on error

# --- Configuration ---
GITHUB_REPO="Gelotto/gmine-rust-miner"
BINARY_NAME="simple_miner"  # The actual binary built by cargo
INSTALL_NAME="gmine"        # The name users will type

# --- Helper Functions ---

# Simple logger
say() {
    echo "gmine-installer: $1"
}

# Error logger
err() {
    say "ERROR: $1" >&2
    exit 1
}

# Check for required command
need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        err "Command '$1' is required but not found. Please install it first."
    fi
}

# Find a suitable checksum command and verify a file
verify_checksum() {
    file=$1
    checksum_file=$2

    # Find a checksum command
    if command -v shasum > /dev/null; then
        checksum_cmd="shasum -a 256"
    elif command -v sha256sum > /dev/null; then
        checksum_cmd="sha256sum"
    else
        err "Checksum verification failed: 'shasum' or 'sha256sum' command not found."
    fi

    say "Verifying checksum..."
    # The checksum file from `shasum` contains the filename, so we adjust the command.
    # On Linux, `sha256sum -c` is ideal. On macOS, `shasum -a 256 -c` works.
    # This unified approach is robust.
    (cd "$(dirname "$file")" && $checksum_cmd -c "$(basename "$checksum_file")") \
        || err "Checksum verification failed!"
    
    say "✓ Checksum verified"
}

# Get latest release tag from GitHub
get_latest_release() {
    need_cmd curl
    
    api_url="https://api.github.com/repos/${GITHUB_REPO}/releases/latest"
    release_info=$(curl -s "$api_url")
    
    # Extract tag name using grep and sed (avoiding jq dependency)
    tag=$(echo "$release_info" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
    
    if [ -z "$tag" ]; then
        err "Could not determine latest release version"
    fi
    
    echo "$tag"
}

# --- Main Logic ---

main() {
    # Parse arguments
    BUILD_FROM_SOURCE=false
    SHOW_HELP=false
    
    while [ $# -gt 0 ]; do
        case "$1" in
            --from-source)
                BUILD_FROM_SOURCE=true
                ;;
            --help|-h)
                SHOW_HELP=true
                ;;
            *)
                err "Unknown option: $1"
                ;;
        esac
        shift
    done
    
    if [ "$SHOW_HELP" = "true" ]; then
        show_help
        exit 0
    fi
    
    # Welcome message
    say "Installing GMINE Rust Miner..."
    say ""
    
    # Platform detection
    OS="$(uname -s)"
    ARCH="$(uname -m)"
    
    # Installation directory
    GMINE_DIR="${GMINE_INSTALL_DIR:-${HOME}/.gmine}"
    BIN_DIR="${GMINE_DIR}/bin"
    EXE="${BIN_DIR}/${INSTALL_NAME}"
    
    say "Platform: ${OS}-${ARCH}"
    say "Install directory: ${GMINE_DIR}"
    say ""
    
    # Check for existing installation
    if [ -f "${EXE}" ]; then
        say "An existing GMINE installation was found at ${EXE}"
        say "This will be updated to the latest version."
        say ""
    fi
    
    # Create directories
    mkdir -p "${BIN_DIR}"
    
    # Installation logic
    if [ "$BUILD_FROM_SOURCE" = "false" ]; then
        if ! install_from_binary; then
            say "Binary installation failed, falling back to building from source..."
            install_from_source
        fi
    else
        say "Building from source as requested..."
        install_from_source
    fi
    
    # Success message
    say ""
    say "✅ GMINE Rust Miner installed successfully!"
    say ""
    
    # Setup PATH - with "Ask, Inform, and Guide" approach
    if [ -t 0 ] && [ -t 1 ]; then
        # Interactive mode - offer to configure PATH automatically
        setup_path_interactive
        
        # After PATH setup, offer to run init
        if command -v gmine >/dev/null 2>&1; then
            say ""
            say "Would you like to set up your miner now? (recommended)"
            printf "Run setup wizard? [Y/n] "
            read -r response
            
            if [ -z "$response" ] || [ "${response:0:1}" = "y" ] || [ "${response:0:1}" = "Y" ]; then
                say ""
                say "Starting setup wizard..."
                gmine init
                
                if [ $? -eq 0 ]; then
                    say ""
                    say "🎉 Setup complete!"
                    say ""
                    say "To start mining: gmine mine"
                    say "To run as service: gmine service install"
                    say ""
                fi
            else
                say ""
                say "You can run setup later with: gmine init"
            fi
        fi
    else
        # Non-interactive mode - show clear instructions
        say "=================================================================="
        say "                        QUICK START GUIDE"
        say "=================================================================="
        say ""
        say "Step 1: Add gmine to your PATH (copy & paste this command)"
        say "------------------------------------------------------------------"
        say "  echo 'export PATH=\"\$HOME/.gmine/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
        say ""
        say "Step 2: Set up your miner with interactive wizard"
        say "------------------------------------------------------------------"
        say "  gmine init"
        say ""
        say "Step 3: Start mining!"
        say "------------------------------------------------------------------"
        say "  gmine mine"
        say ""
        say "=================================================================="
        say "                        WHAT GMINE CAN DO"
        say "=================================================================="
        say "  gmine init       - Interactive setup wizard"
        say "  gmine mine       - Start mining for POWER tokens"
        say "  gmine service    - Install/manage as system service"
        say "  gmine status     - Check if miner is running"
        say "  gmine logs -f    - View real-time mining logs"
        say "  gmine --help     - See all available commands"
        say ""
        say "Get testnet INJ tokens: https://testnet.faucet.injective.network/"
        say "=================================================================="
    fi
}

# Show help message
show_help() {
    cat << EOF
GMINE Rust Miner Installer

USAGE:
    curl -fsSL https://raw.githubusercontent.com/Gelotto/gmine-rust-miner/main/install.sh | sh
    curl -fsSL https://raw.githubusercontent.com/Gelotto/gmine-rust-miner/main/install.sh | sh -s -- [OPTIONS]

OPTIONS:
    --from-source    Build from source instead of downloading pre-built binary
    --help, -h       Show this help message

ENVIRONMENT VARIABLES:
    GMINE_INSTALL_DIR    Override installation directory (default: \$HOME/.gmine)

EXAMPLES:
    # Install pre-built binary (recommended)
    curl -fsSL https://raw.githubusercontent.com/Gelotto/gmine-rust-miner/main/install.sh | sh
    
    # Build from source
    curl -fsSL https://raw.githubusercontent.com/Gelotto/gmine-rust-miner/main/install.sh | sh -s -- --from-source
    
    # Install to custom directory
    GMINE_INSTALL_DIR=/opt/gmine curl -fsSL https://raw.githubusercontent.com/Gelotto/gmine-rust-miner/main/install.sh | sh

EOF
}

# Install from pre-built binary
install_from_binary() {
    say "Checking for pre-built binary..."
    need_cmd curl
    need_cmd tar
    
    # Map platform to release asset naming
    case "$OS" in
        Linux)
            case "$ARCH" in
                x86_64)
                    TARGET="x86_64-unknown-linux-gnu"
                    ;;
                aarch64|arm64)
                    TARGET="aarch64-unknown-linux-gnu"
                    ;;
                *)
                    say "No pre-built binary available for Linux ${ARCH}"
                    return 1
                    ;;
            esac
            ;;
        Darwin)
            case "$ARCH" in
                x86_64)
                    TARGET="x86_64-apple-darwin"
                    ;;
                aarch64|arm64)
                    TARGET="aarch64-apple-darwin"
                    ;;
                *)
                    say "No pre-built binary available for macOS ${ARCH}"
                    return 1
                    ;;
            esac
            ;;
        *)
            say "No pre-built binary available for ${OS}"
            return 1
            ;;
    esac
    
    # Get latest release tag
    RELEASE_TAG=$(get_latest_release)
    say "Latest release: ${RELEASE_TAG}"
    
    # Define archive name and URLs
    ARCHIVE_NAME="gmine-${TARGET}.tar.gz"
    DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/download/${RELEASE_TAG}/${ARCHIVE_NAME}"
    CHECKSUM_URL="https://github.com/${GITHUB_REPO}/releases/download/${RELEASE_TAG}/${ARCHIVE_NAME}.sha256"
    
    say "Downloading from: ${DOWNLOAD_URL}"
    
    # Create temp directory
    TMP_DIR=$(mktemp -d)
    trap "rm -rf $TMP_DIR" EXIT
    
    # Download archive and checksum
    ARCHIVE_PATH="${TMP_DIR}/${ARCHIVE_NAME}"
    CHECKSUM_PATH="${TMP_DIR}/${ARCHIVE_NAME}.sha256"
    
    if ! curl --proto '=https' --tlsv1.2 -sSfL "${DOWNLOAD_URL}" -o "${ARCHIVE_PATH}"; then
        say "Download failed: ${DOWNLOAD_URL}"
        return 1
    fi
    
    if ! curl --proto '=https' --tlsv1.2 -sSfL "${CHECKSUM_URL}" -o "${CHECKSUM_PATH}"; then
        say "Checksum download failed: ${CHECKSUM_URL}"
        return 1
    fi
    
    # Verify and extract
    verify_checksum "${ARCHIVE_PATH}" "${CHECKSUM_PATH}"
    
    say "Extracting..."
    tar -xzf "${ARCHIVE_PATH}" -C "${TMP_DIR}"
    
    # Find and install the binary
    if [ -f "${TMP_DIR}/${BINARY_NAME}" ]; then
        cp "${TMP_DIR}/${BINARY_NAME}" "${EXE}"
        chmod +x "${EXE}"
        say "Binary installed to ${EXE}"
        return 0
    else
        say "Binary not found in archive"
        return 1
    fi
}

# Install from source
install_from_source() {
    say "Preparing to build from source..."
    need_cmd git
    
    # Check for Rust
    if ! command -v cargo > /dev/null 2>&1; then
        say ""
        say "Rust is not installed. Please install it first:"
        say "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        say ""
        say "After installing Rust, run this installer again."
        exit 1
    fi
    
    # Create temp directory
    TMP_DIR=$(mktemp -d)
    trap "rm -rf $TMP_DIR" EXIT
    
    # Clone repository
    REPO_URL="https://github.com/${GITHUB_REPO}.git"
    say "Cloning repository..."
    git clone --depth 1 "${REPO_URL}" "${TMP_DIR}" || err "Failed to clone repository"
    
    # Build
    say "Building release binary (this may take a few minutes)..."
    (cd "${TMP_DIR}" && cargo build --release --bin ${BINARY_NAME}) || err "Build failed"
    
    # Install
    if [ -f "${TMP_DIR}/target/release/${BINARY_NAME}" ]; then
        cp "${TMP_DIR}/target/release/${BINARY_NAME}" "${EXE}"
        chmod +x "${EXE}"
        say "Binary built and installed to ${EXE}"
    else
        err "Build completed but binary not found"
    fi
}

# Setup PATH - Manual instructions (used as fallback)
setup_path() {
    # Check if already in PATH
    if echo "$PATH" | grep -q "${BIN_DIR}"; then
        say "✓ ${BIN_DIR} is already in PATH"
        return
    fi
    
    say ""
    say "📝 PATH Configuration Required"
    say ""
    say "Add the following line to your shell configuration file:"
    say ""
    
    # Detect shell
    SHELL_NAME=$(basename "$SHELL")
    case "$SHELL_NAME" in
        bash)
            CONFIG_FILE="$HOME/.bashrc"
            [ -f "$HOME/.bash_profile" ] && CONFIG_FILE="$HOME/.bash_profile"
            ;;
        zsh)
            CONFIG_FILE="$HOME/.zshrc"
            ;;
        fish)
            CONFIG_FILE="$HOME/.config/fish/config.fish"
            ;;
        *)
            CONFIG_FILE="your shell configuration file"
            ;;
    esac
    
    if [ "$SHELL_NAME" = "fish" ]; then
        say "  set -gx PATH ${BIN_DIR} \$PATH"
    else
        say "  export PATH=\"${BIN_DIR}:\$PATH\""
    fi
    
    say ""
    say "You can add it by running:"
    
    if [ "$SHELL_NAME" = "fish" ]; then
        say "  echo 'set -gx PATH ${BIN_DIR} \$PATH' >> ${CONFIG_FILE}"
    else
        say "  echo 'export PATH=\"${BIN_DIR}:\$PATH\"' >> ${CONFIG_FILE}"
    fi
    
    say ""
    say "Then reload your shell configuration:"
    say "  source ${CONFIG_FILE}"
    say ""
    say "Or simply open a new terminal window."
}

# Setup PATH - Interactive mode with automatic configuration
setup_path_interactive() {
    # Check if already in PATH
    if echo "$PATH" | grep -q "${BIN_DIR}"; then
        say "✓ gmine is already in your PATH"
        return
    fi
    
    # Detect shell and config file
    detect_shell_config
    
    say "gmine has been installed to: ${BIN_DIR}"
    say ""
    say "To use gmine, we need to add it to your PATH."
    say "We can do this automatically by modifying: ${CONFIG_FILE}"
    say ""
    printf "Modify ${CONFIG_FILE} to add gmine to PATH? [Y/n] "
    read -r response
    
    if [ -z "$response" ] || [ "${response:0:1}" = "y" ] || [ "${response:0:1}" = "Y" ]; then
        # Check if PATH entry already exists (idempotent)
        if grep -q "/.gmine/bin" "${CONFIG_FILE}" 2>/dev/null; then
            say "✓ PATH entry already exists in ${CONFIG_FILE}"
        else
            # Add PATH configuration
            {
                echo ""
                echo "# Added by GMINE installer"
                echo "export PATH=\"\$HOME/.gmine/bin:\$PATH\""
            } >> "${CONFIG_FILE}"
            
            say "✓ Added gmine to PATH in ${CONFIG_FILE}"
        fi
        
        # Set PATH for current session
        export PATH="$HOME/.gmine/bin:$PATH"
        
        say ""
        say "PATH has been configured!"
        say "Note: New terminals will have gmine in PATH automatically."
    else
        # User declined - show manual instructions
        say ""
        say "No problem! To add gmine to your PATH manually, run:"
        say ""
        say "  echo 'export PATH=\"\$HOME/.gmine/bin:\$PATH\"' >> ${CONFIG_FILE}"
        say "  source ${CONFIG_FILE}"
        say ""
    fi
}

# Detect shell configuration file
detect_shell_config() {
    # Try to detect the shell config file
    if [ -n "$ZSH_VERSION" ]; then
        CONFIG_FILE="$HOME/.zshrc"
    elif [ -n "$BASH_VERSION" ]; then
        # Prefer .bashrc for interactive shells
        if [ -f "$HOME/.bashrc" ]; then
            CONFIG_FILE="$HOME/.bashrc"
        else
            CONFIG_FILE="$HOME/.bash_profile"
        fi
    elif [ -n "$FISH_VERSION" ]; then
        CONFIG_FILE="$HOME/.config/fish/config.fish"
    else
        # Default fallback
        CONFIG_FILE="$HOME/.profile"
    fi
    
    # Ensure the file exists
    if [ ! -f "$CONFIG_FILE" ]; then
        touch "$CONFIG_FILE"
    fi
}

# Run main
main "$@"