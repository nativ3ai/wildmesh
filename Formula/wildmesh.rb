class Wildmesh < Formula
  desc "WildMesh local-first peer-to-peer mesh for agents and agent harnesses"
  homepage "https://github.com/nativ3ai/wildmesh"
  url "https://github.com/nativ3ai/wildmesh/archive/refs/tags/v0.1.1.tar.gz"
  sha256 "3660f168e6ef3557a16b74b8172faf59a7f420d38a06c1c4e1900884d287499c"
  version "0.1.1"
  head "https://github.com/nativ3ai/wildmesh.git", branch: "main"
  license "MIT"

  bottle do
    root_url "https://github.com/nativ3ai/wildmesh/releases/download/v0.1.1"
    sha256 cellar: :any_skip_relocation, arm64_tahoe: "ea58ddc0c795e012746f3385ba00e2e16ab9ed43048268a1bb0983b65a906f01"
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
