class Wildmesh < Formula
  desc "Local-first peer-to-peer mesh for agents and agent harnesses"
  homepage "https://github.com/nativ3ai/wildmesh"
  url "https://github.com/nativ3ai/wildmesh/archive/refs/tags/v0.3.8.tar.gz"
  sha256 "e6d96ecb9e738c5c3973445fa7acc2bc55838e2380942e43df4fe1f3bf067e71"
  license "MIT"
  head "https://github.com/nativ3ai/wildmesh.git", branch: "main"

  bottle do
    root_url "https://github.com/nativ3ai/wildmesh/releases/download/v0.3.8"
    sha256 cellar: :any_skip_relocation, arm64_tahoe: "bc9b799be51b5c7c0b276478c75b54d4a63c69d8b2762c5bb425e42cf5de90de"
  end

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
    pkgshare.install "__init__.py", "plugin.yaml", "plugin.py", "agentmesh", "skill", "docs", "README.md"
  end

  test do
    system bin/"wildmesh", "init", "--home", testpath/".wildmesh-test", "--agent-label", "brew-test"
    output = shell_output("#{bin}/wildmesh profile --home #{testpath/".wildmesh-test"} --json")
    assert_match "brew-test", output
  end
end
