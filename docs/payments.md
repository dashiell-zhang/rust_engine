# 支付接口

客户端支付模块提供支付宝和微信支付创建入口及异步通知处理。业务先建立订单，再按订单号获取支付地址或调起参数。

## 接入准备

- 在业务配置中设置 AliPay 或 WeiXinPay 模块的应用、商户、密钥和证书参数，具体读取项见[支付服务](../application/application_service/src/pay/service.rs)。
- 配置客户端公开域名与 HTTPS 代理。通知地址来自当前请求的协议和 Host，见[代理与 HTTPS](webapi-network.md)。
- 创建有效业务订单；各支付方式还需满足对应状态、账号绑定或 OpenId 要求。

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

订单、必要配置或对应创建条件不满足时可能返回 204 空正文，调用方应先检查状态码。成功创建不等于已经付款，应等待通知确认。

微信 JSAPI/App 参数缓存 1.8 小时，H5 链接缓存 4.5 分钟，PC 链接缓存十五分钟。多实例应共享业务缓存。

## 支付通知

支付宝通知入口为 POST /Pay/AliPayNotify，使用表单正文验签。微信通知入口为 POST /Pay/WeiXinPayNotify/{mch_id}，保留原始 JSON 正文验签，再解密通知。

通知处理根据有效订单更新支付结果；不要用前端跳转作为订单支付成功的依据。需要扩展履约逻辑时，在业务服务内实现，并考虑重复通知和并发写入。

目前退款及退款查询保留应用契约模板，没有对外退款路由。实际退款业务需补充所需订单规则和结果处理。
