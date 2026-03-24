class Wildmesh < Formula
  desc "WildMesh local-first peer-to-peer mesh for agents and agent harnesses"
  homepage "https://github.com/nativ3ai/wildmesh"
  url "https://github.com/nativ3ai/wildmesh/archive/refs/tags/v0.3.0.tar.gz"
  sha256 "00272979c29ba531cbe365358329e74b2edfe8ff462d491383675b7aebf22233"
  version "0.3.0"
  head "https://github.com/nativ3ai/wildmesh.git", branch: "main"
  license "MIT"

  bottle do
    root_url "https://github.com/nativ3ai/wildmesh/releases/download/v0.3.0"
    sha256 cellar: :any_skip_relocation, arm64_tahoe: "67edbb89f5bdced3e3371d5e26a87dfb5604893660393c8cc149e26c900c9703"
  end

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
    pkgshare.install "plugin.yaml", "plugin.py", "agentmesh", "skill", "docs", "README.md"
  end

  test do
    system "#{bin}/wildmesh", "init", "--home", testpath/".wildmesh-test", "--agent-label", "brew-test"
    output = shell_output("#{bin}/wildmesh profile --home #{testpath/".wildmesh-test"} --json")
    assert_match "brew-test", output
  end
end
