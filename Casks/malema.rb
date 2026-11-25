cask "malema" do
  version "0.5.11"
  sha256 "135ca824deec64f2feb9e5e266623c2407d8248652273602be8de2ecc912f388"

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
