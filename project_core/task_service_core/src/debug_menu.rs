use crate::sync::TaskRuntimeConfiguration;

/// 菜单与事件订阅、已启用任务并行；选择只影响本进程，不保存到数据库。
pub(crate) async fn run(configuration: TaskRuntimeConfiguration) -> Result<bool, String> {
    if !cfg!(debug_assertions) || configuration.menu().is_empty() {
        return Ok(true);
    }
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    // 控制台读取不可取消，不占用 Tokio 阻塞池，防止无输入时拖住运行时退出。
    std::thread::Builder::new()
        .name("task-debug-input".into())
        .spawn(move || {
            loop {
                let mut input = String::new();
                let result = std::io::stdin()
                    .read_line(&mut input)
                    .map(|count| (count, input));
                let end = result.as_ref().map_or(true, |(count, _)| *count == 0);
                if sender.blocking_send(result).is_err() || end {
                    break;
                }
            }
        })
        .map_err(|error| format!("启动任务菜单输入失败: {error}"))?;
    loop {
        let items = configuration.menu();
        println!("\nTaskService Debug 模式（选择仅在当前进程有效）");
        for (index, (kind, name, enabled)) in items.iter().enumerate() {
            println!(
                "[{}] {}：{} {}",
                index + 1,
                kind.display_name(),
                name,
                if *enabled { "已启用" } else { "未启用" }
            );
        }
        println!("输入空格分隔的序号即可启用；回车刷新，start 关闭菜单，quit 停止宿主。");
        let Some(input) = receiver.recv().await else {
            return Ok(true);
        };
        let (count, input) = match input {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(target: "task_service.debug.menu", %error, "读取菜单失败，保留当前选择继续运行");
                return Ok(true);
            }
        };
        if count == 0 {
            return Ok(true);
        }
        match input.trim().to_ascii_lowercase().as_str() {
            "start" | "run" | "y" | "yes" => return Ok(true),
            "skip" | "quit" | "q" | "n" | "no" => return Ok(false),
            _ => {}
        }
        for token in input.split_whitespace() {
            match token
                .parse::<usize>()
                .ok()
                .and_then(|index| index.checked_sub(1))
                .and_then(|index| items.get(index))
            {
                Some((kind, name, _)) => configuration.enable(*kind, name),
                None => println!("无效的任务序号：{token}"),
            }
        }
    }
}
