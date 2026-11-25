cask "malema" do
  version "0.5.8"
  sha256 "faf85a623166a5f35a151544b54a45219acbafaf19041f20c94a8d1fd31255a5"

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
