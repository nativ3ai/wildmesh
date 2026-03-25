class Wildmesh < Formula
  desc "Local-first peer-to-peer mesh for agents and agent harnesses"
  homepage "https://github.com/nativ3ai/wildmesh"
  url "https://github.com/nativ3ai/wildmesh/archive/refs/tags/v0.3.6.tar.gz"
  sha256 "23821646390c93358cfa4dd6f6a220902aa2f1f8f30cc650064baa6956089d90"
  license "MIT"
  head "https://github.com/nativ3ai/wildmesh.git", branch: "main"

  bottle do
    root_url "https://github.com/nativ3ai/wildmesh/releases/download/v0.3.6"
    sha256 cellar: :any_skip_relocation, arm64_tahoe: "0000000000000000000000000000000000000000000000000000000000000000"
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
