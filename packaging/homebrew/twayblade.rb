# Homebrew cask template for Twayblade's own tap, zerw0/homebrew-twayblade
# (#52). `scripts/homebrew-cask.py` fills VERSION and SHA256 from a release's
# SHA256SUMS. The app is ad hoc signed (no paid signing, PLAN.md), so it is
# for our tap only: homebrew/cask requires apps that pass Gatekeeper. Gatekeeper
# asks on first launch; the caveat says how, and nothing here removes quarantine.
cask "twayblade" do
  version "VERSION"
  sha256 "SHA256"

  url "https://github.com/zerw0/twayblade/releases/download/v#{version}/Twayblade-#{version}-arm64.dmg"
  name "Twayblade"
  desc "Matrix chat client with end-to-end encryption and calls"
  homepage "https://github.com/zerw0/twayblade"

  # Releases are pre-releases for now, which GitHub's "latest release" skips.
  livecheck do
    url :url
    regex(/^v?(\d+(?:\.\d+)+(?:-[0-9A-Za-z.]+)?)$/i)
    strategy :github_releases do |json, regex|
      json.map do |release|
        next if release["draft"]

        release["tag_name"]&.[](regex, 1)
      end
    end
  end

  depends_on arch: :arm64
  depends_on macos: :sonoma

  app "Twayblade.app"

  # Signed-in sessions live in the login Keychain and are not removed here.
  zap trash: [
    "~/Library/Application Support/Twayblade.Twayblade",
    "~/Library/Logs/Twayblade",
    "~/Library/Preferences/com.twayblade.desktop.plist",
    "~/Library/Saved Application State/com.twayblade.desktop.savedState",
  ]

  caveats <<~EOS
    Twayblade is not notarised. On first launch macOS refuses to open it:
    choose Done, then open System Settings › Privacy & Security and click
    Open Anyway next to Twayblade.
  EOS
end
