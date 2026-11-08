use crate::*;

use super::budget::SerializationBudget;

/// serde_json 任意精度数字使用的透明 Serde 结构名称
pub(crate) const SERDE_JSON_NUMBER_TOKEN: &str = "$serde_json::private::Number";

/// serde_json 原始 JSON 值使用的透明 Serde 结构名称
pub(crate) const SERDE_JSON_RAW_VALUE_TOKEN: &str = "$serde_json::private::RawValue";

/// 在递归序列化参数时校验复杂度并规范化浮点数
pub(crate) struct CanonicalJsonSerializer<'a, S> {
    serializer: S,
    budget: &'a SerializationBudget,
    depth: usize,
    traversal_depth: usize,
}

impl<'a, S> CanonicalJsonSerializer<'a, S> {
    /// 创建规范 JSON 序列化包装器
    pub(crate) fn new(
        serializer: S,
        budget: &'a SerializationBudget,
        depth: usize,
        traversal_depth: usize,
    ) -> Self {
        Self {
            serializer,
            budget,
            depth,
            traversal_depth,
        }
    }
}

/// 将嵌套值继续交给规范 JSON 序列化器
pub(crate) struct CanonicalJsonValue<'a, 'b, T: ?Sized> {
    value: &'a T,
    budget: &'b SerializationBudget,
    depth: usize,
    traversal_depth: usize,
}

impl<'a, 'b, T: ?Sized> CanonicalJsonValue<'a, 'b, T> {
    /// 创建嵌套值包装器
    pub(crate) fn new(
        value: &'a T,
        budget: &'b SerializationBudget,
        depth: usize,
        traversal_depth: usize,
    ) -> Self {
        Self {
            value,
            budget,
            depth,
            traversal_depth,
        }
    }
}

impl<T> Serialize for CanonicalJsonValue<'_, '_, T>
where
    T: Serialize + ?Sized,
{
    /// 使用规范 JSON 序列化器递归处理当前值
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.budget
            .check_traversal::<S::Error>(self.traversal_depth)?;
        let Some(_snapshot_guard) = self.budget.enter_snapshot(self.value) else {
            return serializer.serialize_none();
        };
        if !self.budget.is_snapshot() {
            self.budget
                .safety_policy
                .validate::<T>()
                .map_err(S::Error::custom)?;
        }
        self.value.serialize(CanonicalJsonSerializer::new(
            serializer,
            self.budget,
            self.depth,
            self.traversal_depth,
        ))
    }
}

impl<'a, S> Serializer for CanonicalJsonSerializer<'a, S>
where
    S: Serializer,
{
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = CanonicalSerializeSeq<'a, S::SerializeSeq>;
    type SerializeTuple = CanonicalSerializeTuple<'a, S::SerializeTuple>;
    type SerializeTupleStruct = CanonicalSerializeTupleStruct<'a, S::SerializeTupleStruct>;
    type SerializeTupleVariant = CanonicalSerializeTupleVariant<'a, S::SerializeTupleVariant>;
    type SerializeMap = CanonicalSerializeMap<'a, S::SerializeMap>;
    type SerializeStruct = CanonicalSerializeStruct<'a, S::SerializeStruct>;
    type SerializeStructVariant = CanonicalSerializeStructVariant<'a, S::SerializeStructVariant>;

    /// 序列化布尔值
    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_bool(value)
    }

    /// 序列化 i8
    fn serialize_i8(self, value: i8) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_i8(value)
    }

    /// 序列化 i16
    fn serialize_i16(self, value: i16) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_i16(value)
    }

    /// 序列化 i32
    fn serialize_i32(self, value: i32) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_i32(value)
    }

    /// 序列化 i64
    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_i64(value)
    }

    /// 序列化 i128
    fn serialize_i128(self, value: i128) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_i128(value)
    }

    /// 序列化 u8
    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_u8(value)
    }

    /// 序列化 u16
    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_u16(value)
    }

    /// 序列化 u32
    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_u32(value)
    }

    /// 序列化 u64
    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_u64(value)
    }

    /// 序列化 u128
    fn serialize_u128(self, value: u128) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_u128(value)
    }

    /// 序列化有限 f32 并统一正负零
    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Self::Error> {
        if !value.is_finite() {
            return Err(<S::Error as SerializeError>::custom(
                "参数包含非有限 f32 浮点数",
            ));
        }

        self.serializer
            .serialize_f32(if value == 0.0 && !self.budget.is_snapshot() {
                0.0
            } else {
                value
            })
    }

    /// 序列化有限 f64 并统一正负零
    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Self::Error> {
        if !value.is_finite() {
            return Err(<S::Error as SerializeError>::custom(
                "参数包含非有限 f64 浮点数",
            ));
        }

        self.serializer
            .serialize_f64(if value == 0.0 && !self.budget.is_snapshot() {
                0.0
            } else {
                value
            })
    }

    /// 序列化字符
    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_char(value)
    }

    /// 序列化字符串
    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_str(value)
    }

    /// 流式格式化并序列化展示值
    fn collect_str<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Display + ?Sized,
    {
        self.serializer.collect_str(value)
    }

    /// 序列化字节
    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok, Self::Error> {
        let child_depth = self.depth.saturating_add(1);
        self.budget
            .enter_many::<S::Error>(child_depth, value.len())?;
        self.serializer.serialize_bytes(value)
    }

    /// 序列化空可选值
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_none()
    }

    /// 序列化非空可选值
    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer.serialize_some(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.depth,
            child_traversal_depth,
        ))
    }

    /// 序列化单元值
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_unit()
    }

    /// 序列化单元结构
    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_unit_struct(name)
    }

    /// 序列化单元枚举变体
    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.serializer
            .serialize_unit_variant(name, variant_index, variant)
    }

    /// 序列化新类型结构
    fn serialize_newtype_struct<T>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer.serialize_newtype_struct(
            name,
            &CanonicalJsonValue::new(value, self.budget, self.depth, child_traversal_depth),
        )
    }

    /// 序列化新类型枚举变体
    fn serialize_newtype_variant<T>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.budget.enter::<S::Error>(child_depth)?;
        self.serializer.serialize_newtype_variant(
            name,
            variant_index,
            variant,
            &CanonicalJsonValue::new(value, self.budget, child_depth, child_traversal_depth),
        )
    }

    /// 开始序列化数组
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer.serialize_seq(len).map(|sequence| {
            CanonicalSerializeSeq::new(sequence, self.budget, child_depth, child_traversal_depth)
        })
    }

    /// 开始序列化元组
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer.serialize_tuple(len).map(|tuple| {
            CanonicalSerializeTuple::new(tuple, self.budget, child_depth, child_traversal_depth)
        })
    }

    /// 开始序列化元组结构
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer
            .serialize_tuple_struct(name, len)
            .map(|tuple| {
                CanonicalSerializeTupleStruct::new(
                    tuple,
                    self.budget,
                    child_depth,
                    child_traversal_depth,
                )
            })
    }

    /// 开始序列化元组枚举变体
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        let container_depth = self.depth.saturating_add(1);
        let child_depth = self.depth.saturating_add(2);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.budget.enter::<S::Error>(container_depth)?;
        self.serializer
            .serialize_tuple_variant(name, variant_index, variant, len)
            .map(|tuple| {
                CanonicalSerializeTupleVariant::new(
                    tuple,
                    self.budget,
                    child_depth,
                    child_traversal_depth,
                )
            })
    }

    /// 开始序列化字典
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer.serialize_map(len).map(|map| {
            CanonicalSerializeMap::new(map, self.budget, child_depth, child_traversal_depth)
        })
    }

    /// 开始序列化结构
    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        let transparent_token = serde_json_transparent_struct_token(name, len);
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer
            .serialize_struct(name, len)
            .map(|structure| {
                CanonicalSerializeStruct::new(
                    structure,
                    self.budget,
                    self.depth,
                    child_depth,
                    child_traversal_depth,
                    transparent_token,
                )
            })
    }

    /// 开始序列化结构枚举变体
    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        let container_depth = self.depth.saturating_add(1);
        let child_depth = self.depth.saturating_add(2);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.budget.enter::<S::Error>(container_depth)?;
        self.serializer
            .serialize_struct_variant(name, variant_index, variant, len)
            .map(|structure| {
                CanonicalSerializeStructVariant::new(
                    structure,
                    self.budget,
                    child_depth,
                    child_traversal_depth,
                )
            })
    }

    /// 返回底层序列化器的人类可读标识
    fn is_human_readable(&self) -> bool {
        self.serializer.is_human_readable()
    }
}

/// 递归规范化数组元素
pub(crate) struct CanonicalSerializeSeq<'a, S> {
    sequence: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeSeq<'a, S> {
    /// 创建数组序列化包装器
    pub(crate) fn new(
        sequence: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            sequence,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeSeq for CanonicalSerializeSeq<'_, S>
where
    S: SerializeSeq,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化数组元素
    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.sequence.serialize_element(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 结束数组序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.sequence.end()
    }
}

/// 递归规范化元组元素
pub(crate) struct CanonicalSerializeTuple<'a, S> {
    tuple: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeTuple<'a, S> {
    /// 创建元组序列化包装器
    pub(crate) fn new(
        tuple: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            tuple,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeTuple for CanonicalSerializeTuple<'_, S>
where
    S: SerializeTuple,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化元组元素
    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.tuple.serialize_element(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 结束元组序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.tuple.end()
    }
}

/// 递归规范化元组结构字段
pub(crate) struct CanonicalSerializeTupleStruct<'a, S> {
    tuple: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeTupleStruct<'a, S> {
    /// 创建元组结构序列化包装器
    pub(crate) fn new(
        tuple: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            tuple,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeTupleStruct for CanonicalSerializeTupleStruct<'_, S>
where
    S: SerializeTupleStruct,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化元组结构字段
    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.tuple.serialize_field(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 结束元组结构序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.tuple.end()
    }
}

/// 递归规范化元组枚举变体字段
pub(crate) struct CanonicalSerializeTupleVariant<'a, S> {
    tuple: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeTupleVariant<'a, S> {
    /// 创建元组枚举变体序列化包装器
    pub(crate) fn new(
        tuple: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            tuple,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeTupleVariant for CanonicalSerializeTupleVariant<'_, S>
where
    S: SerializeTupleVariant,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化元组枚举变体字段
    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.tuple.serialize_field(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 结束元组枚举变体序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.tuple.end()
    }
}

/// 递归规范化字典键和值
pub(crate) struct CanonicalSerializeMap<'a, S> {
    map: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeMap<'a, S> {
    /// 创建字典序列化包装器
    pub(crate) fn new(
        map: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            map,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeMap for CanonicalSerializeMap<'_, S>
where
    S: SerializeMap,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化字典键
    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.map.serialize_key(&CanonicalJsonValue::new(
            key,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 序列化字典值
    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.map.serialize_value(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 结束字典序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.map.end()
    }
}

/// 识别 serde_json 最终输出为单个 JSON 值的私有透明结构协议
pub(crate) fn serde_json_transparent_struct_token(
    name: &'static str,
    len: usize,
) -> Option<&'static str> {
    if len == 1 && matches!(name, SERDE_JSON_NUMBER_TOKEN | SERDE_JSON_RAW_VALUE_TOKEN) {
        Some(name)
    } else {
        None
    }
}

/// 递归规范化结构字段
pub(crate) struct CanonicalSerializeStruct<'a, S> {
    structure: S,
    budget: &'a SerializationBudget,
    parent_depth: usize,
    child_depth: usize,
    child_traversal_depth: usize,
    transparent_token: Option<&'static str>,
}

impl<'a, S> CanonicalSerializeStruct<'a, S> {
    /// 创建结构序列化包装器
    pub(crate) fn new(
        structure: S,
        budget: &'a SerializationBudget,
        parent_depth: usize,
        child_depth: usize,
        child_traversal_depth: usize,
        transparent_token: Option<&'static str>,
    ) -> Self {
        Self {
            structure,
            budget,
            parent_depth,
            child_depth,
            child_traversal_depth,
            transparent_token,
        }
    }
}

impl<S> SerializeStruct for CanonicalSerializeStruct<'_, S>
where
    S: SerializeStruct,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化结构字段
    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        let is_transparent_value = self.transparent_token == Some(key);
        let value_depth = if is_transparent_value {
            self.parent_depth
        } else {
            self.budget.enter::<Self::Error>(self.child_depth)?;
            self.child_depth
        };
        self.structure.serialize_field(
            key,
            &CanonicalJsonValue::new(value, self.budget, value_depth, self.child_traversal_depth),
        )
    }

    /// 结束结构序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.structure.end()
    }
}

/// 递归规范化结构枚举变体字段
pub(crate) struct CanonicalSerializeStructVariant<'a, S> {
    structure: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeStructVariant<'a, S> {
    /// 创建结构枚举变体序列化包装器
    pub(crate) fn new(
        structure: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            structure,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeStructVariant for CanonicalSerializeStructVariant<'_, S>
where
    S: SerializeStructVariant,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化结构枚举变体字段
    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.structure.serialize_field(
            key,
            &CanonicalJsonValue::new(
                value,
                self.budget,
                self.child_depth,
                self.child_traversal_depth,
            ),
        )
    }

    /// 结束结构枚举变体序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.structure.end()
    }
}
