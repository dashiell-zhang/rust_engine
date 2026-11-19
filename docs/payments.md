# 支付接口

客户端支付模块提供支付宝和微信支付创建入口及异步通知处理。业务先建立订单，再按订单号获取支付地址或调起参数。

## 接入准备

- 支付宝网页和小程序分别使用 `AliPayWeb` 和 `AliPayMiniApp` 配置组。每组填写 `AppId`、`AppPrivateKey` 和 `AliPayPublicKey`。
- 微信在 `WeiXinPay` 组填写 `AppId`、`MchId`、`MchApiCertId`、`MchApiCertKey` 和 `MchApiV3Key`。创建请求须能选中唯一商户，同组必需项各有一个非空值。
- 配置客户端公开域名与 HTTPS 代理。通知地址来自当前请求的协议和 Host，见[代理与 HTTPS](webapi-network.md)。
- 订单号必须唯一，订单状态为“待支付”且尚未付款，人民币金额须为正数，最多两位有效小数。

这些创建端点允许匿名访问，按传入的原始订单号查询，不自动检查当前登录用户的订单归属。业务接入时应按自身流程选择开放范围。

## 创建支付

| GET 接口 | 参数 / 结果 |
| --- | --- |
| /Pay/CreateAliPayPC | orderNo，返回支付文本 |
| /Pay/CreateAliPayH5 | orderNo，返回 H5 支付文本 |
| /Pay/CreateAliPayMiniApp | orderNo，使用订单创建人的有效绑定 |
| /Pay/CreateWeiXinPayJSAPI | orderNo、openId，返回调起参数 |
| /Pay/CreateWeiXinPayApp | orderNo，返回 App 调起参数 |
| /Pay/CreateWeiXinPayH5 | orderNo，返回跳转链接 |
| /Pay/CreateWeiXinPayPC | orderNo，返回支付链接 |

例如，在准备好订单和商户配置后：

```powershell
curl.exe "http://127.0.0.1:3001/Pay/CreateWeiXinPayJSAPI?orderNo=ORDER001&openId=替换为用户OpenId"
```

订单不存在或不可支付时返回 204，小程序缺少买家绑定也返回 204。配置缺失、重复或金额无效会返回错误。成功创建不等于已经付款，应等待通知确认。

微信 JSAPI/App 参数缓存 1.8 小时，H5 链接缓存 4.5 分钟，PC 链接缓存十五分钟。多实例应共享业务缓存。

## 支付通知

支付宝通知入口为 POST /Pay/AliPayNotify，使用表单正文验签。微信通知入口为 POST /Pay/WeiXinPayNotify/{mch_id}，保留原始 JSON 正文验签，再解密通知。

支付宝验证 RSA2 签名、应用归属和成功交易状态，处理结果以 HTTP 200 的 `success` 或 `failure` 表示。微信成功返回 204 空正文，失败返回 HTTP 500 和 `{"code":"FAIL","message":"处理失败"}`。微信通知的签名时间与宿主时钟相差须在五分钟内。

以异步通知确认订单付款。重复投递同一笔成功交易会收到成功确认，金额、支付渠道或流水号冲突会返回失败。多实例部署应共用分布式锁和缓存。
