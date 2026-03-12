NAME    = phomemo-macos
VERSION = 1.0.0

DRV_DIR  = drv
PPD_DIR  = build/ppd
PKG_DIR  = build/pkg
ROOT_DIR = build/root
DIST_DIR = dist

DRV_FILES = $(wildcard $(DRV_DIR)/*.drv)
PPD_FILES = $(patsubst $(DRV_DIR)/%.drv,$(PPD_DIR)/%.stamp,$(DRV_FILES))

# Rust filter binaries
ARCH_ARM    = aarch64-apple-darwin
ARCH_X86    = x86_64-apple-darwin
UNIVERSAL   = build/universal
BINARIES    = rastertopm02 rastertopm110 rastertopd30 phomemo-send phomemo-ble

# CUPS install paths
CUPS_BACKEND_DIR = /usr/libexec/cups/backend
CUPS_FILTER_DIR  = /usr/libexec/cups/filter
CUPS_PPD_DIR     = /Library/Printers/PPDs/Contents/Resources

.PHONY: all ppd rust pkg clean install uninstall

all: ppd rust

# --- Rust filter build (universal binaries) ---

RUST_SOURCES = $(wildcard src/*.rs src/bin/*.rs) Cargo.toml Cargo.lock
RUST_STAMP   = build/.rust-stamp

rust: $(RUST_STAMP)

$(RUST_STAMP): $(RUST_SOURCES)
	cargo build --release --features ble --target $(ARCH_ARM)
	cargo build --release --features ble --target $(ARCH_X86)
	mkdir -p $(UNIVERSAL)
	$(foreach bin,$(BINARIES),\
		lipo -create \
			target/$(ARCH_ARM)/release/$(bin) \
			target/$(ARCH_X86)/release/$(bin) \
			-output $(UNIVERSAL)/$(bin);)
	@echo "Ad-hoc signing universal binaries..."
	$(foreach bin,$(BINARIES),codesign -s - -f $(UNIVERSAL)/$(bin);)
	@touch $@

# --- PPD compilation ---

ppd: $(PPD_FILES)

$(PPD_DIR)/%.stamp: $(DRV_DIR)/%.drv | $(PPD_DIR)
	ppdc -d $(PPD_DIR) $<
	@# Patch round-label ImageableArea: ppdc lacks per-media margin support.
	@# Horizontal: 3mm (8.504pt) each side to centre label on stock.
	@# Vertical: 2.5mm (7.087pt) each side (calibrated from test prints).
	@for f in $(PPD_DIR)/*.ppd; do \
		gsed -i \
			-e '/ImageableArea w20h20/s/"[^"]*"/"8.503937 7.086614 65.196850 63.779528"/' \
			-e '/ImageableArea w30h30/s/"[^"]*"/"8.503937 7.086614 93.543307 92.125984"/' \
			-e '/ImageableArea w40h40/s/"[^"]*"/"8.503937 7.086614 121.889764 120.472441"/' \
			"$$f"; \
	done
	@touch $@

$(PPD_DIR):
	mkdir -p $(PPD_DIR)

# --- Package build ---

pkg: ppd $(RUST_STAMP) | $(ROOT_DIR) $(DIST_DIR)
	# Backend
	install -d $(ROOT_DIR)$(CUPS_BACKEND_DIR)
	install -m 700 backend/phomemo-serial $(ROOT_DIR)$(CUPS_BACKEND_DIR)/phomemo-serial
	install -m 700 backend/phomemo-ble-backend $(ROOT_DIR)$(CUPS_BACKEND_DIR)/phomemo-ble
	# Filters (compiled Rust binaries)
	install -d $(ROOT_DIR)$(CUPS_FILTER_DIR)
	install -m 755 $(UNIVERSAL)/rastertopm02  $(ROOT_DIR)$(CUPS_FILTER_DIR)/rastertopm02
	install -m 755 $(UNIVERSAL)/rastertopm110 $(ROOT_DIR)$(CUPS_FILTER_DIR)/rastertopm110
	install -m 755 $(UNIVERSAL)/rastertopd30  $(ROOT_DIR)$(CUPS_FILTER_DIR)/rastertopd30
	install -m 755 $(UNIVERSAL)/phomemo-send  $(ROOT_DIR)$(CUPS_FILTER_DIR)/phomemo-send
	install -m 755 $(UNIVERSAL)/phomemo-ble  $(ROOT_DIR)$(CUPS_FILTER_DIR)/phomemo-ble
	# PPDs
	install -d $(ROOT_DIR)$(CUPS_PPD_DIR)
	install -m 644 $(PPD_DIR)/Phomemo-*.ppd $(ROOT_DIR)$(CUPS_PPD_DIR)/
	# Build component package
	pkgbuild \
		--root $(ROOT_DIR) \
		--identifier com.phomemo.macos.driver \
		--version $(VERSION) \
		--scripts scripts \
		--ownership recommended \
		$(PKG_DIR)/driver.pkg
	# Build product archive
	productbuild \
		--package $(PKG_DIR)/driver.pkg \
		--identifier com.phomemo.macos \
		--version $(VERSION) \
		$(DIST_DIR)/$(NAME)-$(VERSION).pkg

$(ROOT_DIR) $(DIST_DIR):
	mkdir -p $@ $(PKG_DIR)

# --- Direct install (development) ---

install: ppd rust
	sudo install -m 700 backend/phomemo-serial $(CUPS_BACKEND_DIR)/phomemo-serial
	sudo install -m 700 backend/phomemo-ble-backend $(CUPS_BACKEND_DIR)/phomemo-ble
	sudo chown root:wheel $(CUPS_BACKEND_DIR)/phomemo-serial $(CUPS_BACKEND_DIR)/phomemo-ble
	sudo install -m 755 $(UNIVERSAL)/rastertopm02  $(CUPS_FILTER_DIR)/rastertopm02
	sudo install -m 755 $(UNIVERSAL)/rastertopm110 $(CUPS_FILTER_DIR)/rastertopm110
	sudo install -m 755 $(UNIVERSAL)/rastertopd30  $(CUPS_FILTER_DIR)/rastertopd30
	sudo install -m 755 $(UNIVERSAL)/phomemo-send  $(CUPS_FILTER_DIR)/phomemo-send
	sudo install -m 755 $(UNIVERSAL)/phomemo-ble  $(CUPS_FILTER_DIR)/phomemo-ble
	sudo install -m 644 $(PPD_DIR)/Phomemo-*.ppd $(CUPS_PPD_DIR)/
	sudo launchctl kickstart -k system/org.cups.cupsd 2>/dev/null || true
	@echo "Installed. Add a printer with:"
	@echo "  sudo lpadmin -p Phomemo-M110 -E -v 'phomemo-serial:/dev/cu.DEVICE' -P $(CUPS_PPD_DIR)/Phomemo-M110.ppd"

uninstall:
	sudo bash scripts/uninstall.sh

clean:
	rm -rf build dist
	cargo clean
