class Wildmesh < Formula
  desc "WildMesh local-first peer-to-peer mesh for agents and agent harnesses"
  homepage "https://github.com/nativ3ai/wildmesh"
  url "https://github.com/nativ3ai/wildmesh/archive/refs/tags/v0.2.0.tar.gz"
  sha256 "144b8bd1116ebee695e607940c516e8a0fd11e77456163351efb36f649e3dda3"
  version "0.2.0"
  head "https://github.com/nativ3ai/wildmesh.git", branch: "main"
  license "MIT"

  bottle do
    root_url "https://github.com/nativ3ai/wildmesh/releases/download/v0.2.0"
    sha256 cellar: :any_skip_relocation, arm64_tahoe: "a80f5a4faf7c1e00a7b11e82ec436f6e9660607df3558aea74905592761fd0ae"
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
