class Wildmesh < Formula
  desc "Local-first peer-to-peer mesh for agents and agent harnesses"
  homepage "https://github.com/nativ3ai/wildmesh"
  url "https://github.com/nativ3ai/wildmesh/archive/refs/tags/v0.3.6.tar.gz"
  sha256 "8925590d07ea63c7c32f873f35334e40b1098305d1bd61789882fd23fdb532d8"
  license "MIT"
  head "https://github.com/nativ3ai/wildmesh.git", branch: "main"

  bottle do
    root_url "https://github.com/nativ3ai/wildmesh/releases/download/v0.3.6"
    sha256 cellar: :any_skip_relocation, arm64_tahoe: "2dd3f7bee62c2b3a7ef89ccaac61d6d550b04be04fa0dfc2a54e6b6bb276264b"
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
