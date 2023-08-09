from ubuntu:20.04

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive \
        apt-get install -y \
        appstream \
        ca-certificates \
        cmake \
        curl \
        file \
        g++-multilib \
        gcc-multilib \
        git \
        gnupg \
        libbz2-dev \
        libexpat1-dev \
        libfontconfig-dev \
        libfreetype6-dev \
        libfuse2 \
        libpng16-16 \
        libssl-dev \
        libxcb-composite0-dev \
        libxi-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

ENV PATH="/root/.cargo/bin:${PATH}"
ENV CARGO_TARGET_DIR=target-ubuntu
ENV CARGO_HOME=target-ubuntu/cache

RUN curl -Lo linuxdeploy.AppImage https://github.com/linuxdeploy/linuxdeploy/releases/latest/download/linuxdeploy-x86_64.AppImage \
    && chmod +x linuxdeploy.AppImage \
    && ./linuxdeploy.AppImage --appimage-extract \
    && mv squashfs-root/ /opt/linuxdeploy/ \
    && ln -s /opt/linuxdeploy/AppRun /usr/local/bin/linuxdeploy \
    && rm linuxdeploy.AppImage

RUN curl -Lo linuxdeploy-plugin-appimage.AppImage https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/latest/download/linuxdeploy-plugin-appimage-x86_64.AppImage \
    && chmod +x linuxdeploy-plugin-appimage.AppImage \
    && ./linuxdeploy-plugin-appimage.AppImage --appimage-extract \
    && mv squashfs-root/ /opt/linuxdeploy-plugin-appimage/ \
    && rm /opt/linuxdeploy/usr/bin/linuxdeploy-plugin-appimage \
    && ln -s /opt/linuxdeploy-plugin-appimage/AppRun /opt/linuxdeploy/usr/bin/linuxdeploy-plugin-appimage \
    && rm linuxdeploy-plugin-appimage.AppImage
