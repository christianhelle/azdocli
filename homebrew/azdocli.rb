class Azdocli < Formula
  desc "CLI tool for interacting with Azure DevOps"
  homepage "https://christianhelle.com/azdocli"
  version "VERSION_PLACEHOLDER"
  
  if OS.mac? && Hardware::CPU.arm?
    url "https://github.com/christianhelle/azdocli/releases/download/VERSION_PLACEHOLDER/macos-arm64.zip"
    sha256 "SHA256_ARM64_PLACEHOLDER"
  elsif OS.mac? && Hardware::CPU.intel?
    url "https://github.com/christianhelle/azdocli/releases/download/VERSION_PLACEHOLDER/macos-x64.zip"
    sha256 "SHA256_X64_PLACEHOLDER"
  end

  def install
    bin.install "ado" => "ado"
    doc.install "README.md"
    (share/"licenses"/name).install "LICENSE"
  end

  test do
    system "#{bin}/ado", "--help"
  end
end