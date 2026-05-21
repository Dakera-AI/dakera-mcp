#!/usr/bin/env python3
"""Generate dakera-mcp Homebrew formula with real SHA256 checksums.

Usage: python3 scripts/update-homebrew-formula.py <output-path>

Environment variables (set by CI workflow):
  VERSION                         — release version, e.g. 0.10.5
  SHA256_aarch64_apple_darwin     — macOS ARM tarball checksum
  SHA256_x86_64_apple_darwin      — macOS Intel tarball checksum
  SHA256_x86_64_unknown_linux_gnu — Linux x64 tarball checksum
"""
import os
import sys

version  = os.environ["VERSION"]
sha_arm  = os.environ.get("SHA256_aarch64_apple_darwin", "PLACEHOLDER")
sha_x64  = os.environ.get("SHA256_x86_64_apple_darwin", "PLACEHOLDER")
sha_lin  = os.environ.get("SHA256_x86_64_unknown_linux_gnu", "PLACEHOLDER")

# Ruby #{...} interpolation must NOT be Python f-string interpolation.
# Use {{...}} in the f-string to produce literal {version}/{bin} in the output.
formula = f"""\
class DakeraMcp < Formula
  desc "Dakera MCP Server - Model Context Protocol server for AI agent memory"
  homepage "https://dakera.ai"
  version "{version}"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/dakera-ai/dakera-mcp/releases/download/v#{{version}}/dakera-mcp-aarch64-apple-darwin.tar.gz"
      sha256 "{sha_arm}"
    else
      url "https://github.com/dakera-ai/dakera-mcp/releases/download/v#{{version}}/dakera-mcp-x86_64-apple-darwin.tar.gz"
      sha256 "{sha_x64}"
    end
  end

  on_linux do
    url "https://github.com/dakera-ai/dakera-mcp/releases/download/v#{{version}}/dakera-mcp-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "{sha_lin}"
  end

  def install
    bin.install "dakera-mcp"
  end

  test do
    system "#{{bin}}/dakera-mcp", "--version"
  end
end
"""

output = sys.argv[1] if len(sys.argv) > 1 else "Formula/dakera-mcp.rb"
with open(output, "w") as f:
    f.write(formula)

print(f"Updated {output} to v{version}")
print(f"  darwin/arm64 sha256: {sha_arm[:16]}...")
print(f"  darwin/x64   sha256: {sha_x64[:16]}...")
print(f"  linux/x64    sha256: {sha_lin[:16]}...")
