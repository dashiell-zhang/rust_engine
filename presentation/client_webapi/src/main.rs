#[tokio::main]
/// 启动客户端 WebAPI 宿主
async fn main() {
    client_webapi::run()
        .await
        .expect("客户端 WebAPI 宿主运行失败");
}
