cask "malema" do
  version "0.5.12"
  sha256 "3b6fc9c2e0895bddd8bfb870746d2782b4f4a3136c51bbb185158fc625eed450"

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
