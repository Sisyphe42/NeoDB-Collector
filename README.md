# NeoDB Collector

本地优先的媒体记录采集与同步工具。目前已实现基础服务、IMDb CSV 的只读预览 API 和 CLI 命令，**预览 WebUI 与同步写入尚未开放**。当前产品方向是只读来源平台、写入 NeoDB；后续客户端与平台通过独立适配器扩展。

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
| `--preview-retention-days` | `NEODB_COLLECTOR_PREVIEW_RETENTION_DAYS` | `preview.retention_days` | `30` |

YAML 使用 `schema_version: 1`，其中相对路径相对 YAML 文件目录；CLI/环境变量相对路径相对工作目录。只允许 loopback IP（含 `::1`）；不接受公网/LAN 地址或主机名。端口 `0` 表示系统分配空闲端口，实际地址输出在 stderr。

预览保留期为正整数天数，仅用于新任务，不改变已有任务的到期时间。预览运行时已接入启动恢复和每分钟清理，历史任务/结果读取、CSV 上传、恢复及取消/删除 API 和 CLI 命令已开放；预览 UI 尚未开放。重启不会自动重跑任务。清理失败会明确报错并停止服务。`doctor` 不恢复或清理预览任务。

同一数据目录只能由一个 `serve` 进程使用，即使选择不同端口。服务通过 `collector.serve.lock` 文件持有系统锁，退出后自动释放；不要在运行期间删除该文件。

系统数据目录由 `directories::ProjectDirs` 解析（Windows 本地应用数据、macOS Application Support、Linux XDG 数据目录）。可用 `--data-dir` 显式控制位置。SQLite 仅保存操作状态，包括授权散列、撤销状态、单一 NeoDB 端点元数据、映射版本及有保留期的规范化预览记录；不保存明文令牌、原始文件或源 API/页面载荷。配对绑定服务地址，更换端口后需重新配对。

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
| `GET /api/v1/client` | Bearer 客户端查看自身身份，检测撤销 |
| `GET /api/v1/settings/neodb` | WebUI 查看 NeoDB 连接状态 |
| `POST /api/v1/oauth/neodb` / `DELETE /api/v1/oauth/neodb` | WebUI 开始 / 取消只读 OAuth |
| `GET /api/v1/previews/{id}` | 已授权会话或 Bearer 查看历史任务状态、账号绑定及保留期 |
| `GET /api/v1/previews/{id}/results?attempt=N&offset=0&limit=50` | 历史条目证据及字段差异，limit 为 1–100，offset 为原始记录序号 |
| `POST /api/v1/previews/{id}/cancel` | 显式取消指定 attempt，JSON 为 `{"expectedAttempt": 1}` |
| `DELETE /api/v1/previews/{id}` | 删除任务及预览数据，不删除配置或凭据 |
| `POST /api/v1/imports/imdb/ratings?profileId=ID&profileVersion=N` | 原始 UTF-8 `text/csv`，最多 10 MiB；选择已保存的禁用配置版本，202 返回任务回执及跳过数量 |
| `POST /api/v1/previews/{id}/resume` | 显式恢复保留期内的中断/失败/取消任务，JSON 为 `{"expectedAttempt": 1}`，202 返回新 attempt |

健康、能力和契约端点公开；本地管理接口要求 8 小时有效的 HttpOnly / SameSite=Strict 会话，修改操作还要求同源 Origin 与 CSRF 令牌。浏览器请求须同源，Host 仅允许实际监听地址或相同端口的 localhost。不开放 CORS。配对、CLI 认证状态与 NeoDB 只读 OAuth 已可用；WebUI 支持授权、账户状态及取消。令牌仅保存在系统凭据库中。媒体同步尚未接入；配对客户端不能管理 OAuth 或其他客户端。

预览读取要求会话或带 `imports.read` 权限的 Bearer。结果为历史证据，不是当前 NeoDB 状态或写入许可；账号绑定为 current 也不表示旧快照仍新鲜。过期/不存在任务返回 404，attempt 不匹配返回 409；运行中空结果页不表示完成。连接器能力列表在客户端端到端交付前仍为空。

上传/恢复要求同源会话与 CSRF，或带 `imports.submit` 权限的 Bearer。上传不会自动保存/选择最新配置；恢复保持原账号绑定、配置版本和到期时间，重新计算全部记录。202 仅表示持久化接收，不表示完成；断开客户端不会取消已接收工作。提交后的不确定失败返回 `jobId`、`attempt` 与嵌套 `error`，应通过读取接口查询真实状态，不自动重试。共享 TS 客户端保留这份校验后的查询信息。预览可能通过 NeoDB 原生抓取创建公共目录条目，但不修改来源数据或个人标记。

取消/删除要求同源会话与 CSRF，或带 `imports.submit` 权限的 Bearer。返回 `{"applied": true/false}` 表示实际仓库结果，不伪造终止状态；超时/中断后应重新查询任务，不自动重试。取消只影响指定 attempt 的本地工作，不能撤销 NeoDB 已排队的公共目录创建；已有个人标记始终不会被修改。

## CLI 配对

在 WebUI 选择“命令行”，保留名称 `Collector CLI`，生成并复制一次性链接。
PowerShell 从剪贴板传入（macOS 可用 `pbpaste`，Linux 可用剪贴板工具管道）：

```powershell
Get-Clipboard | collector pair
collector status --core-url http://127.0.0.1:19423 --format json
```

自定义名称必须通过 `pair --name "名称"` 与 WebUI 完全对应。配对命令只从标准输入读取链接，
不接受秘密命令行参数。status 默认使用端口 19423；其他端口须显式传入 `--core-url`。
pair/status 不读取 serve/doctor 的 YAML/环境配置。成功退出 0，失败退出 1。
status 确认本地客户端认证，不表示 NeoDB 已授权。

每个 Core 地址保存一个当前 CLI 令牌；重新配对会替换本地选择，旧客户端可在 WebUI 撤销。
若兑换成功但令牌保存失败，命令会提示在 WebUI 撤销该客户端并重新配对；不会将令牌写入文件或输出。

## CLI 导入与预览

先在 WebUI 完成 NeoDB 只读授权、保存禁用的 IMDb 映射版本，并为 CLI 配对。
以下命令默认连接 `http://127.0.0.1:19423`；不同端口用 `--core-url` 指定。

```sh
collector import imdb-ratings ratings.csv --profile-id imdb_ratings --profile-version 1 --format json
collector preview status JOB_ID --format json
collector preview results JOB_ID --attempt 1 --offset 0 --limit 50 --format json
collector preview cancel JOB_ID --expected-attempt 1 --format json
collector preview resume JOB_ID --expected-attempt 1 --format json
collector preview delete JOB_ID --format json
```

`FILE` 为文件路径或 `-`（管道标准输入），最多 10 MiB；CLI 原样发送 CSV，不自行解析。
配置版本、结果 attempt、取消/恢复的 expected-attempt 必须显式指定。
上传/恢复回执只表示接收成功，不表示处理完成；取消/删除返回实际 `applied` 值。
结果每次只读取一页，offset 是原始记录序号；运行中空页不表示完成。
历史证据包含字段建议、既有值及未知/空值区别，不是写入许可。

CLI 不自动轮询或重试；上传/恢复失败若带 `jobId`、`attempt`，请查询真实状态。
响应丢失也不意味着回滚。预览可能创建公共目录条目，但不会修改个人标记或来源平台。
`--format json` 输出 API DTO，成功退出 0（包括 `applied=false`），失败退出 1；参数语法错误退出 2。
客户端命令不读取服务 YAML/环境配置，也不读取数据库或保存原始 CSV。

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
crates/collector-connectors  内置适配器：IMDb CSV 解析、NeoDB OAuth/目录/标记读取传输
crates/collector-storage     SQLx / SQLite 适配器
crates/collector-server      Axum HTTP 与静态 WebUI
crates/collector-cli         配置、诊断、进程生命周期
packages/contracts-ts       从 Rust 生成的类型与运行时校验器
packages/core-client-ts     版本化、带响应校验的 HTTP 客户端
apps/webui                  React 状态页
contracts/                  生成的 OpenAPI 与 JSON Schema
```

浏览器扩展、桌面壳、n8n node、Playnite 插件与同步业务留在后续里程碑；不提前创建空包。持久化产品决定以 [.trellis/spec/product/collector-spec.yaml](.trellis/spec/product/collector-spec.yaml) 为准，当前实现契约见 [.trellis/spec/backend/foundation-runtime.md](.trellis/spec/backend/foundation-runtime.md)。
