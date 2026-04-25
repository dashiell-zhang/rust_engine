# syntax=docker/dockerfile:1.7

## 开发与构建共用的基础镜像
FROM rust:latest AS dev-base
WORKDIR /workspace
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

## 编译发布版并准备运行时所需的默认配置
FROM dev-base AS builder
WORKDIR /workspace
COPY . .
RUN cargo build --locked --release -p admin_webapi
RUN mkdir -p /runtime-assets/presentation/admin_webapi \
    && cp presentation/admin_webapi/appsettings.toml /runtime-assets/presentation/admin_webapi/appsettings.toml

## 运行镜像只保留可执行文件和默认配置
FROM ubuntu:latest AS runtime
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tzdata \
    && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /app/logs /data/storage/admin_webapi
EXPOSE 3000
COPY --from=builder /workspace/target/release/admin_webapi /usr/local/bin/admin_webapi
COPY --from=builder /runtime-assets/ /app/
ENTRYPOINT ["/bin/sh", "-lc", "exec /usr/local/bin/admin_webapi \"$@\"", "--"]

## 导出发布目录时只保留二进制
FROM busybox:latest AS bundle
RUN mkdir -p /bundle
COPY --from=builder /workspace/target/release/admin_webapi /bundle/admin_webapi

## 导出目标使用空镜像避免带入额外文件
FROM scratch AS artifact
COPY --from=bundle /bundle/ /