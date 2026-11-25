cask "malema" do
  version "0.5.6"
  sha256 "50b5556e932f50f84b95f0f3fb56ef111584432f6ad3c03a1f060e8f709ebb6a"

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
