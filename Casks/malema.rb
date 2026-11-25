cask "malema" do
  version "0.5.13"
  sha256 "2952d4e98440d1b22c840ede96dc021ff24d14aa14337fbae69fa6e8a6474faa"

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
