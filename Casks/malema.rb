cask "malema" do
  version "0.5.7"
  sha256 "4f9c4d4b22c10482db84af54a768b5f4f1ecdc4f9a48c607842e92868db6f9b0"

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
