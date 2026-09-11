#!/usr/bin/env bash
#
# LON oracle - macOS prerequisite check/install script
# -----------------------------------------------------------
# Run this BEFORE bootstrap_mac.sh. Only checks/installs the toolchain; does
# not perform any deploy/account operations, safe to run repeatedly.
#
# Usage:
#   chmod +x check_prereqs_mac.sh
#   ./check_prereqs_mac.sh

set -uo pipefail

OK=0
MISSING=0

ok()   { echo "   [OK]      $1"; OK=$((OK+1)); }
warn() { echo "   [MISSING] $1"; MISSING=$((MISSING+1)); }
info() { echo "   [INFO]    $1"; }

echo "== Homebrew =="
if command -v brew >/dev/null 2>&1; then
    ok "Homebrew installed ($(brew --version | head -1))"
else
    warn "Homebrew not found."
    echo '        Install: /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"'
fi

echo ""
echo "== Xcode Command Line Tools (git, make, cc) =="
if xcode-select -p >/dev/null 2>&1; then
    ok "Command Line Tools installed"
else
    warn "Command Line Tools missing."
    echo "        Install: xcode-select --install"
fi

echo ""
echo "== Rust / Cargo =="
if command -v cargo >/dev/null 2>&1; then
    ok "cargo installed ($(cargo --version))"
else
    warn "Rust/cargo not found."
    echo '        Install: curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh'
fi

echo ""
echo "== protoc (protobuf-compiler) =="
if command -v protoc >/dev/null 2>&1; then
    ok "protoc installed ($(protoc --version))"
else
    warn "protoc not found."
    echo "        Install: brew install protobuf"
fi

echo ""
echo "== unzip =="
if command -v unzip >/dev/null 2>&1; then
    ok "unzip installed"
else
    warn "unzip not found."
    echo "        Install: brew install unzip"
fi

echo ""
echo "== Python 3 =="
if command -v python3 >/dev/null 2>&1; then
    ok "python3 installed ($(python3 --version))"
else
    warn "python3 not found."
    echo "        Install: brew install python3"
fi

echo ""
echo "== Docker =="
if command -v docker >/dev/null 2>&1; then
    ok "docker CLI installed ($(docker --version))"
    if docker info >/dev/null 2>&1; then
        ok "Docker daemon is running"
    else
        warn "Docker is installed but the daemon isn't running (open Docker Desktop)."
    fi
    if docker compose version >/dev/null 2>&1; then
        ok "docker compose (v2 plugin) available ($(docker compose version))"
    else
        warn "docker compose not found - Docker Desktop may be outdated."
    fi
else
    warn "Docker not found."
    echo "        Install: download Docker Desktop from https://www.docker.com/products/docker-desktop/, install it, open it."
fi

echo ""
echo "== RISC0 (rzup / cargo risczero) =="
export PATH="$HOME/.risc0/bin:$PATH"
if command -v rzup >/dev/null 2>&1; then
    ok "rzup installed"
else
    warn "rzup not found."
    echo "        Install: curl -L https://risczero.com/install | bash   (then: rzup install)"
fi
if command -v cargo >/dev/null 2>&1 && cargo risczero --version >/dev/null 2>&1; then
    ok "cargo risczero toolchain ready ($(cargo risczero --version))"
else
    warn "cargo risczero toolchain not found (rzup install may not have been run)."
fi

echo ""
echo "== Docker image pull (the bedrock node image is pulled, not built) =="
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    if docker pull --quiet hello-world >/dev/null 2>&1; then
        ok "Docker can pull images from the registry"
        docker rmi hello-world >/dev/null 2>&1 || true
    else
        warn "Docker cannot pull images (network/DNS/proxy issue)."
        echo "        The bedrock node image comes from ghcr.io, so this must work."
        echo "        If you are on a VPN or corporate proxy, try turning it off."
    fi
else
    info "Skipped (Docker not available)"
fi

echo ""
echo "== Disk / RAM (rough check) =="
FREE_GB=$(df -g "$HOME" 2>/dev/null | tail -1 | awk '{print $4}')
if [ -n "${FREE_GB:-}" ] && [ "$FREE_GB" -lt 20 ]; then
    warn "Free disk space looks low (~${FREE_GB}GB). Recommend 20GB+ for RISC0 builds and docker images."
else
    ok "Disk space looks sufficient (~${FREE_GB:-?}GB free)"
fi
TOTAL_RAM_GB=$(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1024 / 1024 / 1024 ))
if [ "$TOTAL_RAM_GB" -gt 0 ] && [ "$TOTAL_RAM_GB" -lt 16 ]; then
    warn "Total RAM ~${TOTAL_RAM_GB}GB. 16GB+ is recommended for RISC0 guest builds, but it may still work."
else
    info "Total RAM: ~${TOTAL_RAM_GB}GB"
fi

echo ""
echo "================================================================"
if [ "$MISSING" -eq 0 ]; then
    echo " Everything is ready ($OK/$OK checks passed). You can run bootstrap_mac.sh."
else
    echo " $MISSING item(s) missing, $OK check(s) passed."
    echo " Run the install commands above, then run this script again."
fi
echo "================================================================"

[ "$MISSING" -eq 0 ]
