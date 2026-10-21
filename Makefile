CC ?= clang
CURL_CONFIG ?= curl-config

CPPFLAGS += -Iinclude
CFLAGS += -std=c11 -Wall -Wextra -Wpedantic -Wconversion -Wshadow -Wstrict-prototypes -MMD -MP
LDLIBS += $(shell $(CURL_CONFIG) --libs)

BUILD_DIR := build
APP := $(BUILD_DIR)/groov
TEST_APP := $(BUILD_DIR)/test_vk
SOURCES := src/main.c src/vk.c
OBJECTS := $(SOURCES:%.c=$(BUILD_DIR)/%.o)
TEST_OBJECTS := $(BUILD_DIR)/src/vk.o $(BUILD_DIR)/tests/test_vk.o
DEPENDS := $(OBJECTS:.o=.d) $(TEST_OBJECTS:.o=.d)

.PHONY: all clean run test

all: $(APP)

$(APP): $(OBJECTS)
	@mkdir -p $(@D)
	$(CC) $(OBJECTS) $(LDLIBS) -o $@

$(TEST_APP): $(TEST_OBJECTS)
	@mkdir -p $(@D)
	$(CC) $(TEST_OBJECTS) $(LDLIBS) -o $@

$(BUILD_DIR)/%.o: %.c
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(CFLAGS) -c $< -o $@

test: $(TEST_APP)
	$(TEST_APP)

run: $(APP)
	$(APP) --help

clean:
	rm -rf $(BUILD_DIR)

-include $(DEPENDS)
