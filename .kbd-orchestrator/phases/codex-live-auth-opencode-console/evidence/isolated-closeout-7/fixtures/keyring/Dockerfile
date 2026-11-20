FROM debian:bookworm-slim
RUN apt-get update -qq && apt-get install -qq -y --no-install-recommends dbus-daemon gnome-keyring libsecret-tools python3 openssl >/dev/null && rm -rf /var/lib/apt/lists/*
