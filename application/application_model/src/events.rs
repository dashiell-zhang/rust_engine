/// 通知任务宿主重新读取队列和定时任务配置。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TaskSettingChangedEvent {}

impl event_bus::Event for TaskSettingChangedEvent {
    const NAME: &'static str = "Application.Model.Events.TaskSettingChangedEvent";
}
