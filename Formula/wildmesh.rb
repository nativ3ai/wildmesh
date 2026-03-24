class Wildmesh < Formula
  desc "WildMesh local-first peer-to-peer mesh for agents and agent harnesses"
  homepage "https://github.com/nativ3ai/wildmesh"
  url "https://github.com/nativ3ai/wildmesh/archive/refs/tags/v0.2.1.tar.gz"
  sha256 "1e3c023b515afb174b9234682b7705c3aacc08041090df8618fb6317571d2bc9"
  version "0.2.1"
  head "https://github.com/nativ3ai/wildmesh.git", branch: "main"
  license "MIT"

  bottle do
    root_url "https://github.com/nativ3ai/wildmesh/releases/download/v0.2.1"
    sha256 cellar: :any_skip_relocation, arm64_tahoe: "cffdf8569d08370a6066d6900e75f766f46bd42ab898bd301bc97f552924c02f"
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
