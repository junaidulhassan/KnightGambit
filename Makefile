PREFIX ?= /usr/local
DESTDIR ?=
APP_ID := io.github.knightgambit.KnightGambit
CARGO ?= cargo
TARGET_DIR ?= $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),target)
ICON_ROOT := $(DESTDIR)$(PREFIX)/share/icons/hicolor

.PHONY: all build test run install uninstall deb

all: build

build:
	$(CARGO) build --release

test:
	$(CARGO) test

run:
	$(CARGO) run --release

install: build
	install -Dm755 $(TARGET_DIR)/release/knightgambit $(DESTDIR)$(PREFIX)/bin/knightgambit
	install -Dm644 data/$(APP_ID).desktop $(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop
	install -Dm644 data/$(APP_ID).metainfo.xml $(DESTDIR)$(PREFIX)/share/metainfo/$(APP_ID).metainfo.xml
	install -Dm644 data/icons/hicolor/scalable/apps/$(APP_ID).svg $(ICON_ROOT)/scalable/apps/$(APP_ID).svg
	install -Dm644 data/icons/hicolor/symbolic/apps/$(APP_ID)-symbolic.svg $(ICON_ROOT)/symbolic/apps/$(APP_ID)-symbolic.svg
	for icon in data/icons/hicolor/scalable/actions/*.svg; do \
		install -Dm644 $$icon $(ICON_ROOT)/scalable/actions/$$(basename $$icon); \
	done
	@if [ -z "$(DESTDIR)" ]; then \
		gtk-update-icon-cache -q -t -f $(ICON_ROOT) 2>/dev/null || true; \
		update-desktop-database -q $(PREFIX)/share/applications 2>/dev/null || true; \
	fi

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/knightgambit
	rm -f $(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop
	rm -f $(DESTDIR)$(PREFIX)/share/metainfo/$(APP_ID).metainfo.xml
	rm -f $(ICON_ROOT)/scalable/apps/$(APP_ID).svg
	rm -f $(ICON_ROOT)/symbolic/apps/$(APP_ID)-symbolic.svg
	rm -f $(ICON_ROOT)/scalable/actions/kg-flag-symbolic.svg
	rm -f $(ICON_ROOT)/scalable/actions/kg-draw-symbolic.svg
	rm -f $(ICON_ROOT)/scalable/actions/kg-engine-symbolic.svg
	rm -f $(ICON_ROOT)/scalable/actions/kg-trophy-symbolic.svg
	rm -f $(ICON_ROOT)/scalable/actions/kg-checkmate-symbolic.svg
	rm -f $(ICON_ROOT)/scalable/actions/kg-review-symbolic.svg
	rm -f $(ICON_ROOT)/scalable/actions/kg-history-symbolic.svg
	rm -f $(ICON_ROOT)/scalable/actions/kg-stats-symbolic.svg
	rm -f $(ICON_ROOT)/scalable/actions/kg-book-symbolic.svg
	rm -f $(ICON_ROOT)/scalable/actions/kg-filter-symbolic.svg

deb:
	./packaging/build-deb.sh
