#[tokio::main]
/// 启动管理端 WebAPI 宿主
async fn main() {
    admin_webapi::run()
        .await
        .expect("管理端 WebAPI 宿主运行失败");
}
