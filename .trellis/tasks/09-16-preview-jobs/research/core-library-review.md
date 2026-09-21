# Core 通用库复用审查

审查日期：2026-09-18。基线：`de64aa9`。
状态：复用原则及 CSRF 替换已提交；独立的内部秘密类型迁移已完成并通过验证。
OAuth 协议仍待评估。下表“现状”为初次审查基线，后续落地以相应 spec 为准。
范围：Core 的 access / OAuth / mapping / catalog resolver，以及对应的
HTTP、凭据和网络适配器。不是完整安全审计，也不声称发现了可利用漏洞。

## 结论

不应将“Core 目前只依赖 domain”升级为“Core 永远不得使用通用库”。
判断依据应是净收益、维护状态、依赖能力、测试和边界，而不是依赖数量。
同时，现有不少代码本来就在使用标准库或由适配器调用成熟库；不能把所有
业务编排都当作重复造轮子。

## 逐项结果

| 范围及源码 | 现状 | 建议 |
|---|---|---|
| `collector-server/src/access.rs:159` | 手写 XOR 累积比较 CSRF 字节 | 优先改用 `subtle::ConstantTimeEq`；保留明确的长度策略和错误语义 |
| `collector-core/src/oauth.rs:11`、`:17`；`access.rs:59` | app secret、access token 和 SecretStore 接口使用普通 String | 评估 `secrecy::SecretString`，显式暴露明文；先定义内部秘密类型边界，再逐段迁移 |
| `collector-connectors/src/neodb.rs:140`、`:161` | 手工构造授权 URL 和 token 请求 | `oauth2` 可承担协议构造与解析；值得验证，但不能替代应用注册、`/api/me`、权限约束和 Core 生命周期 |
| `collector-core/src/oauth.rs:279`、`:312` | state 消费、单例流程、凭据替换和清理 | 保留业务编排；不引入状态机框架来包装四个状态 |
| `collector-core/src/mapping.rs:44`、`:51`、`:244` | `is_finite`、范围检查、`clamp`、HashMap/HashSet、少量映射分支 | 已在使用标准库；暂不加入数值库、通用表达式引擎或校验框架 |
| `collector-core/src/catalog_resolver.rs`、Server `resolution_control.rs` | 已用 Tokio timeout/select/sleep 和 CancellationToken | 已完成复用；Core 仅保留业务策略和控制端口 |
| `collector-storage/src/credentials.rs:22`、`:28` | getrandom、SHA-256、base64；OS keyring | 已复用，不需要重写或改用 JWT |
| `collector-connectors/src/neodb.rs:43` | `url::Url` 解析加应用限制 | 保留 URL 库；HTTPS/loopback、同源、路径允许范围仍是业务安全规则 |
| `tools/boundaries.mjs` | Core 的精确依赖白名单只有 collector-domain | 保留可执行白名单，但允许经审查的纯通用库逐项加入；不要直接改成允许任意依赖 |

## 优先级和实施边界

1. **小切片：替换 CSRF 手写比较。** `subtle` 放在 Server，避免为了这一处
   调整 Core 白名单。测试相同/不同/短/长/缺失 token，以及既有 Origin、
   session、Bearer 不降级约束。此处是减少自定义安全原语，不是宣称当前有
   已证实的时序攻击。
2. **独立切片：内部秘密类型。** 审查 Core/适配器上的 token、app secret、
   OAuth code 和临时复制。采用 secrecy 时显式调整允许依赖，默认不启用秘密
   序列化。公开 API 中本就必须交付一次的 token 不应被误改成不可用的字段。
   保留 OS 凭据库、数据库仅存引用和错误脱敏规则；测试编译期序列化约束、
   debug 输出及全部 OAuth 失败/回滚路径。
3. **条件切片：oauth2 协议适配。** 先证明能保留当前表单认证、read-only scope、
   10 秒超时、64 KiB 流式上限、禁止重定向和脱敏错误；不能直接使用库默认
   HTTP 路径绕过现有限制。不要顺便改变 PKCE、refresh、撤销或授权范围。
   若适配层净复杂度没有下降，记录比较结果后保留现状。
4. **不做的替换。** 不为简单格式谓词加入 regex；不为数值缩放加入数学框架；
   不为四状态 OAuth 加状态机库；不为现有固定轮询引入重试框架。

后续修改必须保持跨平台、Core 无网络/数据库/运行时绑定、公开契约兼容。
各切片单独测试和提交；认证类型迁移不应夹在搜索功能提交中。

## 风险说明

### 内部秘密类型复用评估（2026-09-21）

- 采用 `secrecy` 0.10.3，Core 重导出其 SecretString/ExposeSecret，不自制包装层。
  本地下载的 crate manifest 标注 MIT OR Apache-2.0、Rust 1.60、无 build script；
  该版本声明的最低版本不代表 workspace 的最低 Rust 版本。
- 启用 `serde` 以直接反序列化受保护字段；这不提供 SecretString 的 Serialize。
  必需传递依赖为 zeroize，serde 为所选可选依赖，两者已在锁文件中；仅新增 secrecy。
  不引入网络、运行时或平台绑定。当前官方文档与可获取的发布包是本次维护依据，
  不据此保证未来维护或宣称所有平台已测试。
- 净收益：内部凭据端口和 OAuth 值统一类型，明文访问显式；身份读取借用而非复制
  token。代价是适配器和 mock 的签名迁移。HTTP DTO、一次性交付及状态机不变。
- 以 Core 编译失败 doctest 阻止隐式 Serialize/Display，以 Debug 和适配器/回滚
  测试守住行为边界。仍不宣称所有 HTTP、解析中间缓冲区或系统内存被清零。

### 适用限制

- `secrecy` 使访问显式、避免意外 debug 输出并清理其拥有的秘密内存；不会
  自动清理 HTTP 库、解析中间缓冲区、此前克隆或所有系统副本。
- `subtle` 提供抗编译器优化的常数时间工具，但官方明确为尽力而为，不能
  以单元测试证明端到端恒定时序。
- OAuth state 当前先计算 SHA-256 再比较哈希；不能把普通哈希相等比较
  直接描述为明文 token 的前缀泄漏，也不应据此凭空报告漏洞。
- 现有配置解析已经使用 serde/serde-saphyr，数据库和凭据库已有 SQLx/keyring；
  Core 的端口声明本身不是这些库的重复实现。

## 官方依据

- [subtle 2.6.1](https://docs.rs/subtle/2.6.1/subtle/)：ConstantTimeEq 和适用限制。
- [secrecy 0.10.3](https://docs.rs/secrecy/0.10.3/secrecy/)：显式访问、debug 防泄漏、zeroize、默认不提供秘密序列化。
- [oauth2 5.0.0](https://docs.rs/oauth2/5.0.0/oauth2/)：协议客户端、自定义 HTTP 接口、禁用重定向和秘密比较注意事项。
- [url](https://docs.rs/url/latest/url/)：URL 解析基础设施。

初次审查仅检查代码及官方文档，没有调用实站。后续批准的工程原则已进入
`backend/repository-layout.md`，CSRF 行为进入 `backend/state-and-pairing.md`。
本切片直接复用锁文件中已有的 subtle 2.6.1（BSD-3-Clause，纯 Rust，
不引入网络/OS/运行时能力）；只需要字节比较，因此不启用默认 std/i128
功能。现有传递依赖可能合并启用这些功能，不应宣称整个依赖图已关闭它们。
维护依据为上述官方版本文档；当前工具链及 Windows 门禁验证兼容性，
不据此宣称新增的最低 Rust 版本或其他平台已经实测。无公开契约变化。
