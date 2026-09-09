.DEFAULT_GOAL := web

BUILD_ROOT := $(CURDIR)/.build
WEB_TARGET := $(BUILD_ROOT)/web/cargo
ANDROID_TARGET := $(BUILD_ROOT)/android/cargo
NATIVE_TARGET := $(BUILD_ROOT)/native/cargo

.PHONY: web web-check web-wasm android-check native-check clean-web clean-android clean-native clean

# Default build: browser/WASM only. It does not compile desktop or Android.
web: web-wasm
	cd apps/engine-web && npm run build

web-check: web-wasm
	cd apps/engine-web && npm exec tsc -- -b --pretty false

web-wasm:
	cd apps/engine-web && npm run build:wasm

android-check:
	CARGO_TARGET_DIR=$(ANDROID_TARGET) cargo check -p rebook-android-host --target aarch64-linux-android

native-check:
	CARGO_TARGET_DIR=$(NATIVE_TARGET) cargo check -p rebook-desktop

clean-web:
	cargo clean --target-dir $(WEB_TARGET)
	rm -rf apps/engine-web/dist apps/engine-web/pkg crates/engine-wasm/pkg

clean-android:
	cargo clean --target-dir $(ANDROID_TARGET)

clean-native:
	cargo clean --target-dir $(NATIVE_TARGET)

clean: clean-web clean-android clean-native
	rm -rf target
