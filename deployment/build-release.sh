#!/bin/sh
# 在 Rust 开发容器或流水线构建容器中执行。
set -eu

if [ "$#" -ne 1 ]; then
    echo "用法：sh deployment/build-release.sh admin_webapi|client_webapi|task_service" >&2
    exit 1
fi
package=$1
case "$package" in
    admin_webapi|client_webapi|task_service) ;;
    *) echo "不支持的发布宿主：$package" >&2; exit 1 ;;
esac

workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$workspace"
build_target=x86_64-unknown-linux-gnu
cargo build --locked --release --target "$build_target" -p "$package"
target_directory=${CARGO_TARGET_DIR:-target}
binary="$target_directory/$build_target/release/$package"

# 发布包完整生成后再替换，三个宿主拥有独立目录。
mkdir -p dist
output_root=$(CDPATH= cd dist && pwd -P)
if [ "$output_root" != "$workspace/dist" ]; then
    echo "dist 必须位于当前工作区内，不能是外部目录链接" >&2
    exit 1
fi
output="$output_root/$package"
if [ -L "$output" ] || { [ -e "$output" ] && [ ! -d "$output" ]; }; then
    echo "发布目标必须是普通目录" >&2
    exit 1
fi
staging=$(mktemp -d "$output_root/.$package.XXXXXX")
backup="$staging.previous"
trap 'if [ -n "$staging" ] && [ -d "$staging" ]; then rm -rf -- "$staging"; fi' EXIT
trap 'exit 1' HUP INT TERM

install -m 755 "$binary" "$staging/$package"
if [ "$package" != task_service ]; then
    mkdir -p "$staging/licenses"
    install -m 644 project_core/webapi_core/ASP.NET-Core-LICENSE.txt "$staging/licenses/ASP.NET-Core-LICENSE.txt"
fi
if [ "$package" = admin_webapi ]; then
    mkdir -p "$staging/InitData"
    install -m 640 InitData/AuthorizeInitData.json "$staging/InitData/AuthorizeInitData.json"
fi
configuration_directory="$staging/presentation/$package"
mkdir -p "$configuration_directory"
install -m 640 "presentation/$package/appsettings.toml" "$configuration_directory/appsettings.toml"
if [ -f "presentation/$package/appsettings.Production.toml" ]; then
    install -m 640 "presentation/$package/appsettings.Production.toml" "$configuration_directory/appsettings.Production.toml"
fi
rustc -vV > "$staging/build-info.txt"
getconf GNU_LIBC_VERSION >> "$staging/build-info.txt"
ldd "$binary" > "$staging/runtime-libraries.txt"

if [ -d "$output" ]; then
    mv -- "$output" "$backup"
fi
if ! mv -- "$staging" "$output"; then
    if [ -d "$backup" ]; then
        mv -- "$backup" "$output" || echo "恢复失败，旧发布包保留在 $backup" >&2
    fi
    exit 1
fi
staging=
if [ -d "$backup" ]; then
    rm -rf -- "$backup"
fi
echo "发布包已生成到 $output"
