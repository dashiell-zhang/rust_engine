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
RUN cargo build --locked --release -p task_service
RUN mkdir -p /runtime-assets/presentation/task_service \
    && cp presentation/task_service/appsettings.toml /runtime-assets/presentation/task_service/appsettings.toml

## 运行镜像保留任务服务所需的目录与默认配置
FROM ubuntu:latest AS runtime
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tzdata \
    && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /app/logs /data/storage/admin_webapi /data/storage/client_webapi /data/storage/task_service
COPY --from=builder /workspace/target/release/task_service /usr/local/bin/task_service
COPY --from=builder /runtime-assets/ /app/
ENTRYPOINT ["/bin/sh", "-lc", "exec /usr/local/bin/task_service \"$@\"", "--"]

## 导出发布目录时只保留二进制
FROM busybox:latest AS bundle
RUN mkdir -p /bundle
COPY --from=builder /workspace/target/release/task_service /bundle/task_service

## 导出目标使用空镜像避免带入额外文件
FROM scratch AS artifact
COPY --from=bundle /bundle/ /