# Cosmic Samba Manager

default: build-release

build:
    cargo build

build-release:
    cargo build --release

run:
    cargo run

install:
    cargo build --release
    sudo install -Dm755 target/release/cosmic-samba /usr/bin/cosmic-samba
    sudo install -Dm644 data/com.krmobdev.CosmicSamba.desktop /usr/share/applications/com.krmobdev.CosmicSamba.desktop
    sudo install -Dm644 data/icon.svg /usr/share/icons/hicolor/scalable/apps/com.krmobdev.CosmicSamba.svg
    sudo gtk-update-icon-cache /usr/share/icons/hicolor || true

clean:
    cargo clean
