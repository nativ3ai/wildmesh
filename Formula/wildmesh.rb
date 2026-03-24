class Wildmesh < Formula
  desc "WildMesh local-first peer-to-peer mesh for agents and agent harnesses"
  homepage "https://github.com/nativ3ai/wildmesh"
  url "https://github.com/nativ3ai/wildmesh/archive/refs/tags/v0.2.3.tar.gz"
  sha256 "bdd3a4a7d03d8dbb646f7a792138b00c1e3e31ba6bcfeb73df47cd3101f32c31"
  version "0.2.3"
  head "https://github.com/nativ3ai/wildmesh.git", branch: "main"
  license "MIT"

  bottle do
    root_url "https://github.com/nativ3ai/wildmesh/releases/download/v0.2.3"
    sha256 cellar: :any_skip_relocation, arm64_tahoe: "0345dd232c543debf6c9a3a1b777edacb63807cf67caa2073054bb6a97c10683"
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
