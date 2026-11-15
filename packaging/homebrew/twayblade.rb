# Homebrew cask template for Twayblade (#52). Not published: it belongs in our
# own tap first, then homebrew/cask once eligible. VERSION and SHA256 come
# from the release's DMG and SHA256SUMS; the cask needs a Developer ID signed,
# notarised DMG before it can ship, since Homebrew no longer accepts casks for
# apps that Gatekeeper rejects.
cask "twayblade" do
  version "VERSION"
  sha256 "SHA256"

  url "https://github.com/zerw0/twayblade/releases/download/v#{version}/Twayblade-#{version}-arm64.dmg"
  name "Twayblade"
  desc "Matrix chat client with end-to-end encryption and calls"
  homepage "https://github.com/zerw0/twayblade"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on arch: :arm64
  depends_on macos: ">= :sonoma"

  app "Twayblade.app"

  # Signed-in sessions live in the login Keychain and are not removed here.
  zap trash: [
    "~/Library/Application Support/Twayblade.Twayblade",
    "~/Library/Preferences/com.twayblade.desktop.plist",
  ]
end
