# typed: strict
# frozen_string_literal: true

# Homebrew formula for phomemo-macos CUPS driver.
#
# Tap setup:
#   1. Create a repo named homebrew-phomemo under your GitHub account
#   2. Copy this file to Formula/phomemo-macos.rb in that repo
#   3. Update the sha256 after creating the release tag:
#        curl -sL https://github.com/jacquesg/phomemo-macos/archive/refs/tags/v1.0.0.tar.gz | shasum -a 256
#   4. Users install with:
#        brew install jacquesg/phomemo/phomemo-macos

class PhomemoMacos < Formula
  desc "CUPS driver for Phomemo thermal label printers"
  homepage "https://github.com/jacquesg/phomemo-macos"
  url "https://github.com/jacquesg/phomemo-macos/archive/refs/tags/v1.0.0.tar.gz"
  sha256 "PLACEHOLDER"
  license "MIT"

  depends_on macos: :monterey
  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--features", "ble"

    %w[rastertopm02 rastertopm110 rastertopd30 phomemo-send phomemo-ble].each do |name|
      libexec.install "target/release/#{name}"
    end

    ppd_build = buildpath/"build/ppd"
    mkdir_p ppd_build
    Dir[buildpath/"drv/*.drv"].each do |drv|
      system "ppdc", "-d", ppd_build, drv
    end
    (share/"phomemo/ppd").install Dir[ppd_build/"*.ppd"]

    (share/"phomemo/backend").install "backend/phomemo-serial"
    (share/"phomemo/backend").install "backend/phomemo-ble-backend" => "phomemo-ble"

    setup = bin/"phomemo-cups-setup"
    setup.write cups_setup_script
    setup.chmod 0755
  end

  def caveats
    <<~EOS
      To complete the installation, copy the driver files to the
      CUPS directories and restart CUPS:

        sudo phomemo-cups-setup

      To remove the driver from CUPS (run before `brew uninstall`):

        sudo phomemo-cups-setup --uninstall
    EOS
  end

  test do
    assert_predicate libexec/"rastertopm110", :executable?
    assert_predicate libexec/"rastertopm02", :executable?
    assert_predicate libexec/"rastertopd30", :executable?
  end

  private

  def cups_setup_script
    <<~BASH
      #!/bin/bash
      set -e

      PREFIX="#{opt_libexec}"
      SHARE="#{opt_share}/phomemo"
      CUPS_BACKEND="/usr/libexec/cups/backend"
      CUPS_FILTER="/usr/libexec/cups/filter"
      CUPS_PPD="/Library/Printers/PPDs/Contents/Resources"

      if [ "$1" = "--uninstall" ]; then
        if [ "$(id -u)" -ne 0 ]; then
          echo "error: must run as root (use sudo)" >&2
          exit 1
        fi

        echo "Removing Phomemo CUPS printers..."
        for printer in $(lpstat -p 2>/dev/null | awk '/^printer Phomemo/ {print $2}'); do
          echo "  Removing $printer"
          lpadmin -x "$printer" 2>/dev/null || true
        done

        echo "Removing CUPS backends..."
        rm -f "$CUPS_BACKEND/phomemo-serial"
        rm -f "$CUPS_BACKEND/phomemo-ble"

        echo "Removing CUPS filters..."
        rm -f "$CUPS_FILTER/rastertopm02"
        rm -f "$CUPS_FILTER/rastertopm110"
        rm -f "$CUPS_FILTER/rastertopd30"
        rm -f "$CUPS_FILTER/phomemo-send"
        rm -f "$CUPS_FILTER/phomemo-ble"

        echo "Removing PPD files..."
        rm -f "$CUPS_PPD"/Phomemo-*.ppd

        echo "Restarting CUPS..."
        launchctl kickstart -k system/org.cups.cupsd 2>/dev/null || true

        echo "Done. Run 'brew uninstall phomemo-macos' to remove the formula."
        exit 0
      fi

      if [ "$(id -u)" -ne 0 ]; then
        echo "error: must run as root (use sudo)" >&2
        exit 1
      fi

      echo "Installing Phomemo CUPS driver..."

      install -m 700 "$SHARE/backend/phomemo-serial" "$CUPS_BACKEND/phomemo-serial"
      install -m 700 "$SHARE/backend/phomemo-ble"    "$CUPS_BACKEND/phomemo-ble"
      chown root:wheel "$CUPS_BACKEND/phomemo-serial" "$CUPS_BACKEND/phomemo-ble"

      install -m 755 "$PREFIX/rastertopm02"  "$CUPS_FILTER/rastertopm02"
      install -m 755 "$PREFIX/rastertopm110" "$CUPS_FILTER/rastertopm110"
      install -m 755 "$PREFIX/rastertopd30"  "$CUPS_FILTER/rastertopd30"
      install -m 755 "$PREFIX/phomemo-send"  "$CUPS_FILTER/phomemo-send"
      install -m 755 "$PREFIX/phomemo-ble"   "$CUPS_FILTER/phomemo-ble"

      install -m 644 "$SHARE/ppd/"*.ppd "$CUPS_PPD/"

      echo "Restarting CUPS..."
      launchctl kickstart -k system/org.cups.cupsd 2>/dev/null || true

      echo "Done. Add your printer via System Settings > Printers & Scanners,"
      echo "or with lpadmin (see the README for examples)."
    BASH
  end
end
