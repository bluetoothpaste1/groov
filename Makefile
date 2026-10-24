CC ?= clang
CURL_CONFIG ?= curl-config
CARGO ?= cargo
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
MANDIR ?= $(PREFIX)/share/man/man1

RUST_APP := target/debug/groov
LEGACY_APP := build/groov-legacy
TEST_APP := build/test_vk
LEGACY_OBJECTS := build/legacy-main.o build/vk.o
TEST_OBJECTS := build/vk.o build/test_vk.o

.PHONY: all build release install uninstall clean run test legacy test-vk docs-check

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

legacy: $(LEGACY_APP)

$(LEGACY_APP): $(LEGACY_OBJECTS)
	@mkdir -p $(@D)
	$(CC) $(LEGACY_OBJECTS) $(shell $(CURL_CONFIG) --libs) -o $@

$(TEST_APP): $(TEST_OBJECTS)
	@mkdir -p $(@D)
	$(CC) $(TEST_OBJECTS) $(shell $(CURL_CONFIG) --libs) -o $@

build/legacy-main.o: src/main.c
	@mkdir -p $(@D)
	$(CC) -Iinclude -std=c11 -Wall -Wextra -Wpedantic -Wconversion -Wshadow -Wstrict-prototypes -MMD -MP -c $< -o $@

build/vk.o: src/vk.c
	@mkdir -p $(@D)
	$(CC) -Iinclude -std=c11 -Wall -Wextra -Wpedantic -Wconversion -Wshadow -Wstrict-prototypes -MMD -MP -c $< -o $@

build/test_vk.o: tests/test_vk.c
	@mkdir -p $(@D)
	$(CC) -Iinclude -std=c11 -Wall -Wextra -Wpedantic -Wconversion -Wshadow -Wstrict-prototypes -MMD -MP -c $< -o $@

test: test-vk
	$(CARGO) test --locked

test-vk: $(TEST_APP)
	$(TEST_APP)

docs-check:
	python3 scripts/check_docs.py

run: build
	$(RUST_APP) help

clean:
	rm -rf build target

-include build/*.d
