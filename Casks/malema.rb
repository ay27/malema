cask "malema" do
  version "0.5.10"
  sha256 "f996e7596b86a1879446e5251cbfe360610adf642a381beb62413242ec69743d"

  url "https://github.com/ay27/malema/releases/download/v#{version}/malema-macos-arm64.zip"
  name "Malema"
  name "骂了吗"
  desc "Track how often you swear at AI coding tools"
  homepage "https://malema.app/"

  auto_updates true
  depends_on :macos
  depends_on arch: :arm64

  app "骂了吗.app"
end
