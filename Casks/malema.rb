cask "malema" do
  version "0.5.9"
  sha256 "315cb9e8d4e0e9c4449dfff1d53860a6a7430b7bf2f1a3a06202965996025276"

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
