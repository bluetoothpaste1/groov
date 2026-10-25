CARGO ?= cargo
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
MANDIR ?= $(PREFIX)/share/man/man1

RUST_APP := target/debug/groov

.PHONY: all build release install uninstall clean run test module-tests docs-check

all: build

build: $(RUST_APP)

$(RUST_APP): Cargo.toml Cargo.lock $(wildcard src/*.rs)
	$(CARGO) build --locked

release:
	$(CARGO) build --release --locked

install: release
	@mkdir -p "$(BINDIR)"
	install -m 755 target/release/groov "$(BINDIR)/groov"
	@if [ -f packaging/groov.1 ]; then \
		mkdir -p "$(MANDIR)"; \
		install -m 644 packaging/groov.1 "$(MANDIR)/groov.1"; \
	fi

uninstall:
	rm -f "$(BINDIR)/groov" "$(MANDIR)/groov.1"

test:
	$(CARGO) test --locked
	$(MAKE) module-tests

module-tests:
	python3 basic_modules/speedr/test_speedr.py

docs-check:
	python3 scripts/check_docs.py

run: build
	$(RUST_APP) help

clean:
	rm -rf build target
