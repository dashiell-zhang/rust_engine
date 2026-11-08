# syntax=docker/dockerfile:1.7

## 开发与构建共用的基础镜像
FROM rust:latest AS dev-base
WORKDIR /workspace
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates clang libclang-dev python3 ninja-build libfontconfig1-dev fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*

## 编译发布版并准备运行时所需的默认配置
FROM dev-base AS builder
WORKDIR /workspace
COPY . .
RUN cargo build --locked --release -p task_service
RUN mkdir -p /runtime-assets/presentation/task_service \
    && cp presentation/task_service/appsettings.toml /runtime-assets/presentation/task_service/appsettings.toml \
    && if [ -f presentation/task_service/appsettings.Production.toml ]; then \
        cp presentation/task_service/appsettings.Production.toml /runtime-assets/presentation/task_service/appsettings.Production.toml; \
    fi

## 运行镜像保留任务服务所需的目录与默认配置
FROM ubuntu:latest AS runtime
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tzdata libstdc++6 libfontconfig1 fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /app/Logs /data/storage/admin_webapi /data/storage/client_webapi /data/storage/task_service
COPY --from=builder /workspace/target/release/task_service /usr/local/bin/task_service
COPY --from=builder /runtime-assets/ /app/
ENTRYPOINT ["/bin/sh", "-lc", "exec /usr/local/bin/task_service \"$@\"", "--"]

## 导出可独立启动的发布目录，配置路径与默认宿主入口一致
FROM busybox:latest AS bundle
RUN mkdir -p /bundle
COPY --from=builder /workspace/target/release/task_service /bundle/task_service
COPY --from=builder /runtime-assets/ /bundle/

## 导出目标使用空镜像避免带入额外文件
FROM scratch AS artifact
COPY --from=bundle /bundle/ /
