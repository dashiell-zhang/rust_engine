# 基础数据与图片接口

两个 WebAPI 的 `/Base` 接口通过 `application_interface::services::base::BaseService` 查询基础数据。地区和字典使用 `[database].read_connection` 指向的只读连接，留空时连接主库但保持会话只读；这些查询不保存应用层结果缓存。

| GET 接口 | 参数与返回 |
| --- | --- |
| `/Base/GetRegion` | `provinceId`、`cityId` 均省略或为 0 时返回省；非零 `provinceId` 返回城市；非零 `cityId` 返回区县，并优先于省份条件 |
| `/Base/GetRegionAll` | 返回省、市、区三级树；地区 ID 为 JSON 字符串，叶子 `childList` 为 null，没有下级记录的父节点为数组 `[]` |
| `/Base/GetValueList` | `groupId` 省略时为 0，只返回 `Dictionary` 模块中该组的有效键值；重复键会报错 |
| `/Base/GetSnowflakeId` | 返回 JSON 字符串形式的雪花 ID，例如 `"320000000000000001"` |
| `/Base/GetQrCode` | 必填 `text`，返回 500×500 PNG，使用 L 纠错级别 |
| `/Base/GetVerifyCode` | 仅客户端提供；`sign` 是 GUID，省略时使用全零 GUID，返回 PNG 验证码图片 |

地区和字典查询排除软删除记录。省市区查询只选择指定层级，不自动校验非零城市是否属于同时传入的省份；不存在的筛选值返回空列表。

图形验证码为四位非零开头数字，有效期五分钟，保存在宿主缓存中的逻辑键 `VerifyCode{sign}` 下。同一个标记再次请求会覆盖验证码并重新开始计时。调用方应为每次验证流程分配独立 GUID，并使用相同标记完成后续校验。

管理端基础接口需要请求签名，Debug 构建按现有签名规则放行；客户端基础接口公开访问。二维码和验证码返回 `image/png`，其他结果返回 `application/json`。
