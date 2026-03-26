class Wildmesh < Formula
  desc "Local-first peer-to-peer mesh for agents and agent harnesses"
  homepage "https://github.com/nativ3ai/wildmesh"
  url "https://github.com/nativ3ai/wildmesh/archive/refs/tags/v0.3.9.tar.gz"
  sha256 "bf7fb9b68998aae0a46c14774e04601a5db629839d515f56490a94de34fecfd9"
  license "MIT"
  head "https://github.com/nativ3ai/wildmesh.git", branch: "main"

  bottle do
    root_url "https://github.com/nativ3ai/wildmesh/releases/download/v0.3.9"
    sha256 cellar: :any_skip_relocation, arm64_tahoe: "3d5de266509e1a4bccc1d6a7a7a3562eca096bb9e6bf530dc77750e1973e0e71"
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
