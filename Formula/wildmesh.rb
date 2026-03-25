class Wildmesh < Formula
  desc "Local-first peer-to-peer mesh for agents and agent harnesses"
  homepage "https://github.com/nativ3ai/wildmesh"
  url "https://github.com/nativ3ai/wildmesh/archive/refs/tags/v0.3.3.tar.gz"
  sha256 "cb0b16bf6a159f47a81dea2a5c3cb90598677619d363a1de496858df771f139a"
  license "MIT"
  head "https://github.com/nativ3ai/wildmesh.git", branch: "main"

  bottle do
    root_url "https://github.com/nativ3ai/wildmesh/releases/download/v0.3.3"
    sha256 cellar: :any_skip_relocation, arm64_tahoe: "c8d4c4510d8e84bfcb622ae35f008a5af0a2f700c5e12c9843fc64cc14d8f518"
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
