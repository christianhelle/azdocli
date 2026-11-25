class Azdocli < Formula
  desc "CLI tool for interacting with Azure DevOps"
  homepage "https://christianhelle.com/azdocli"
  version "VERSION_PLACEHOLDER"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/christianhelle/azdocli/releases/download/VERSION_PLACEHOLDER/macos-arm64.zip"
      sha256 "SHA256_MACOS_ARM64_PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/christianhelle/azdocli/releases/download/VERSION_PLACEHOLDER/macos-x64.zip"
      sha256 "SHA256_MACOS_X64_PLACEHOLDER"
    end
  end

  on_linux do
    depends_on "openssl@3"

    on_arm do
      url "https://github.com/christianhelle/azdocli/releases/download/VERSION_PLACEHOLDER/linux-arm64.zip"
      sha256 "SHA256_LINUX_ARM64_PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/christianhelle/azdocli/releases/download/VERSION_PLACEHOLDER/linux-x64.zip"
      sha256 "SHA256_LINUX_X64_PLACEHOLDER"
    end
  end

  def install
    bin.install "azdocli"
    doc.install "README.md"
    (share/"licenses"/name).install "LICENSE"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/azdocli --version")
  end
end
