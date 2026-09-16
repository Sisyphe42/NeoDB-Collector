# NeoDB Collector

本地优先的媒体记录采集与同步工具。目前已实现可运行基础层，**尚不支持平台接入、导入或同步**。当前产品方向是只读来源平台、写入 NeoDB；后续客户端与平台通过独立适配器扩展。

## 运行

需要 Rust stable（含 rustfmt、clippy 和本机 C/C++ 编译工具）、Node.js 24+、pnpm 10.33.0。依赖版本由 Cargo.lock 和 pnpm-lock.yaml 固定。所有包暂不发布，项目许可仍待确定。

在仓库根目录运行：

```sh
pnpm install --frozen-lockfile
pnpm build
cargo run --locked -p collector-cli -- serve
```

打开终端输出的 `Open WebUI` 一次性链接（10 分钟有效），可管理本地客户端配对与撤销；链接中的密钥会立即从地址栏移除。直接打开 `http://127.0.0.1:19423` 仍可查看公共状态。WebUI 与 API 使用同一端口。Ctrl-C 关闭服务；Unix 也支持 SIGTERM。缺少前端产物时 API 仍可用，首页会提示构建 WebUI。

```sh
cargo run --locked -p collector-cli -- doctor --format json
cargo run --locked -p collector-cli -- serve --config collector.example.yaml
cargo run --locked -p collector-cli -- serve --port 19424 --data-dir ./local-data
```

`doctor` 返回配置、目录、数据库、端口和 WebUI 检查结果，失败退出码为 1。注意：它会创建数据目录并迁移 `collector.db`；不是只读诊断。已有服务占用同一端口时，端口检查会报告不可用。

## 配置

参考 [collector.example.yaml](collector.example.yaml)。配置文件必须显式传入，不会自动扫描当前目录。

优先级：CLI 参数 > `NEODB_COLLECTOR_*` 环境变量 > YAML > 默认值。

| 参数 | 环境变量 | YAML 字段 | 默认值 |
|---|---|---|---|
| `--config` | `NEODB_COLLECTOR_CONFIG` | — | 无配置文件 |
| `--bind` | `NEODB_COLLECTOR_BIND` | `server.bind` | `127.0.0.1` |
| `--port` | `NEODB_COLLECTOR_PORT` | `server.port` | `19423` |
| `--data-dir` | `NEODB_COLLECTOR_DATA_DIR` | `storage.data_dir` | 系统应用数据目录 |
| `--assets-dir` | `NEODB_COLLECTOR_ASSETS_DIR` | `webui.assets_dir` | `apps/webui/dist` |

YAML 使用 `schema_version: 1`，其中相对路径相对 YAML 文件目录；CLI/环境变量相对路径相对工作目录。只允许 loopback IP（含 `::1`）；不接受公网/LAN 地址或主机名。端口 `0` 表示系统分配空闲端口，实际地址输出在 stderr。

系统数据目录由 `directories::ProjectDirs` 解析（Windows 本地应用数据、macOS Application Support、Linux XDG 数据目录）。可用 `--data-dir` 显式控制位置。SQLite 保存迁移账本、授权散列与撤销状态，以及预留的单一 NeoDB 端点元数据；不保存明文令牌或源平台数据。配对绑定服务地址，更换端口后需重新配对。

## 当前 API

| 路由 | 返回 |
|---|---|
| `GET /api/v1/health` | Core/API/契约版本、真实数据库健康状态；异常为 503 |
| `GET /api/v1/capabilities` | 空连接器列表、媒体类型、已实现路由 |
| `GET /api/v1/schema` | OpenAPI 3.1 |
| `POST /api/v1/session/bootstrap` | 兑换一次性链接，设置 HttpOnly 会话 |
| `GET /api/v1/session` / `DELETE /api/v1/session` | 查看会话 / 退出 |
| `POST /api/v1/pairings` | 已授权 WebUI 创建一次性配对链接 |
| `POST /api/v1/pairings/exchange` | 客户端兑换绑定名称及类型的 Bearer 令牌 |
| `GET /api/v1/clients` / `DELETE /api/v1/clients/{id}` | WebUI 查看 / 撤销客户端 |

健康、能力和契约端点公开；本地管理接口要求 8 小时有效的 HttpOnly / SameSite=Strict 会话，修改操作还要求同源 Origin 与 CSRF 令牌。浏览器请求须同源，Host 仅允许实际监听地址或相同端口的 localhost。不开放 CORS。配对 API 已可用，但 CLI 配对命令、NeoDB OAuth 和平台读写接口尚未接入；配对客户端不能管理 OAuth 或其他客户端。

## 开发与验证

```sh
pnpm contracts:generate  # 修改 Rust DTO / 路由后更新生成产物
pnpm contracts:check     # 只比对，不覆盖契约文件
pnpm exec playwright install chromium
pnpm verify             # Rust + TS + 契约 + 构建 + 真实进程/浏览器 smoke
```

Linux 构建还需要 `libdbus-1-dev` 和 `pkg-config`，用于系统凭据库适配；CI 使用 `pnpm exec playwright install --with-deps chromium` 安装浏览器系统依赖。测试创建独立临时数据库，退出时清理；页面截图保存在忽略的 `test-results/`。测试不访问真实系统凭据或平台账户。Windows smoke 结束进程；跨平台服务器单元测试另行验证优雅关闭和端口释放。

开发时可先启动 Core，再运行 `pnpm --filter @neodb-collector/webui dev` 查看公共状态；授权操作请使用 Core 托管的构建产物，Vite 不绕过 Origin / CSRF 检查。

## 代码边界

```text
crates/collector-domain      Rust DTO 与版本常量，无 I/O
crates/collector-core        用例与端口，不依赖具体运行时
crates/collector-connectors  内置平台网络适配器；当前仅有尚未接入路由的 NeoDB 只读 OAuth 传输
crates/collector-storage     SQLx / SQLite 适配器
crates/collector-server      Axum HTTP 与静态 WebUI
crates/collector-cli         配置、诊断、进程生命周期
packages/contracts-ts       从 Rust 生成的类型与运行时校验器
packages/core-client-ts     版本化、带响应校验的 HTTP 客户端
apps/webui                  React 状态页
contracts/                  生成的 OpenAPI 与 JSON Schema
```

浏览器扩展、桌面壳、n8n node、Playnite 插件与同步业务留在后续里程碑；不提前创建空包。持久化产品决定以 [.trellis/spec/product/collector-spec.yaml](.trellis/spec/product/collector-spec.yaml) 为准，当前实现契约见 [.trellis/spec/backend/foundation-runtime.md](.trellis/spec/backend/foundation-runtime.md)。
