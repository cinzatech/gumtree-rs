CARGO   ?= cargo
PREFIX  ?= /usr/local
BINDIR  ?= $(PREFIX)/bin
BIN     := diffame

RELEASE_BIN = target/release/$(BIN)

.PHONY: all check fmt lint test build clean install uninstall

all: check build

check: fmt lint test

fmt:
	$(CARGO) fmt -- --check

lint:
	$(CARGO) clippy --all-targets -- -D warnings

test:
	$(CARGO) test

build:
	$(CARGO) build --release

install:
	@test -f $(RELEASE_BIN) || { echo "error: $(RELEASE_BIN) not found, run 'make build' first"; exit 1; }
	install -d $(DESTDIR)$(BINDIR)
	install -m 755 $(RELEASE_BIN) $(DESTDIR)$(BINDIR)/$(BIN)

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/$(BIN)

clean:
	$(CARGO) clean
