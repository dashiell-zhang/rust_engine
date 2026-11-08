//! 宿主显式装配的共享资源，只按类型保存，不负责创建或依赖解析。
use std::{
    any::{Any, TypeId},
    collections::HashMap,
};
#[derive(Default)]
pub struct Resources {
    values: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}
impl Resources {
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) -> Result<(), String> {
        let key = TypeId::of::<T>();
        if self.values.contains_key(&key) {
            return Err(format!("资源重复注册: {}", std::any::type_name::<T>()));
        }
        self.values.insert(key, Box::new(value));
        Ok(())
    }
    pub fn get<T: Send + Sync + 'static>(&self) -> Result<&T, String> {
        self.values
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref())
            .ok_or_else(|| format!("资源未注册: {}", std::any::type_name::<T>()))
    }
}
