# GitHub 登录与云同步方案（一期）

> 状态：**已批准**（2026-10-20 用户确认；稿图 19 号同批批准）。
> 流程位置：本文档是「文档 → 稿图 → 视觉基线 → 生产代码」的第一步；确认本文档后才画稿图（登录 UI、账号面板），稿图审批后才动生产代码。
> 参考研究：TengriQaghan（Electron + React 媒体中心，`D:\TengriQaghan\tengriqaghan`，只读参考）的 GitHub 登录与 Gist 同步实现，2026-10-20 深读结论，教训直接移植。

---

## 一、背景与动机

Phoenix-DRX 是桌面应用，用户可能多台电脑使用。当前所有偏好只存在本机：

| 数据 | 位置 | 现状 |
| --- | --- | --- |
| 音效开关 | localStorage（`src/lib/sound.ts`） | 仅本机 |
| 动效开关 | localStorage（`src/lib/motion.ts`） | 仅本机 |
| 界面语言 | localStorage（`src/lib/i18n/i18n.svelte.ts`） | 仅本机 |
| 已读公告 | localStorage（`src/lib/announcements.svelte.ts`） | 仅本机 |

换机即丢、逐台重配。目标：**GitHub 账号登录后，这类轻量偏好自动跨设备同步**——零自建后端、零运维成本，与本项目「免费开源」铁律一致（GitHub 账号即用户身份，无私有服务器、无账号系统、无付费墙）。

TengriQaghan 已在同一思路下跑通完整链路，并踩过真实事故（Gist 串号、token 存储并发、同步回环等），本方案整体移植其成熟模式，技术栈映射为 Tauri 2。

## 二、一期范围（收窄版）

### 2.1 做什么

1. **GitHub OAuth Device Flow 登录**：应用展示设备码，用户在浏览器输入授权，无需内嵌 WebView。
2. **同步现有真实存在的 4 项设置**：音效开关、动效开关、界面语言、已读公告。
3. **私有 Gist 云存储 + 字段级合并 + 防回环**。
4. **设置页账号面板**：登录状态、手动立即同步、登出。

### 2.2 不做什么（一期明确排除）

| 排除项 | 理由 |
| --- | --- |
| 存档本体（profile.json）上云 | 体积大、含用户私密数据、SPT 数据责任重；一期只同步轻量偏好 |
| 编辑器预设 / 收藏方案 / 对比快照同步 | **功能本身尚不存在**（诚实原则，不放假开关）；等落地后按 §九 逐项纳入 |
| 手机号 + 密码通道 | TengriQaghan 第二通道的动机是国内访问 GitHub 慢；一期不做，Rust 侧留 provider 抽象 |
| 多账号 / 账号切换 | 一期单 GitHub 账号，登出重登即覆盖 |

### 2.3 schema 扩展位

Gist JSON 预留 `schemaVersion` 与开放的 `settings` 键空间：未来功能（预设/收藏/快照）落地后往里加键即可，不破坏一期已同步数据。

---

## 三、技术选型

### 3.1 登录：GitHub OAuth Device Flow

- 端点：`POST https://github.com/login/device/code` 取设备码 → `POST https://github.com/login/oauth/access_token` 轮询换 token。
- scope：`gist read:user`（最小够用——Gist 读写 + 读用户名）。
- client_id：公开硬编码，无需 client_secret（Device Flow 型 OAuth App）；**用户侧前置准备**：在 GitHub 新建 OAuth App 并勾选 Device Flow（§八）。
- 轮询节奏按首次响应的 `interval` 秒；遇 `slow_down` 间隔 +5s；`authorization_pending` 继续等；网络异常线性退避重试 8 次；超过 `expires_in`（约 15 分钟）或用户取消即结束。
- token 无刷新概念：失效（401）兜底 = 清场后重新登录，不做静默续期。

### 3.2 云端存储：私有 Gist 单文件 JSON

- 每账号一个**私有 Gist**（`public: false`），单文件 `phoenix-drx-sync.json`。
- 首登发现：拉当前账号 gists 列表，按「文件名 + 描述标记」匹配归属；没有则创建，找到则复用，gist id 存本机凭据库（后续免发现）。
- 选型理由：GitHub 账号体系自带、免费、无私有服务器；TengriQaghan 同模式已长期验证。

### 3.3 凭据存储：Rust keyring

- token 只存 `keyring` crate（Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service），等效 TengriQaghan 的 Electron safeStorage。
- **前端永远拿不到 token**：所有 GitHub 请求在 Rust 侧发出。
- gist id 同存凭据库，与账号绑定。

---

## 四、Rust 侧设计

### 4.1 模块划分（src-tauri/src/ 新增）

- `auth.rs` —— Device Flow 全流程 + token 凭据库存取 + 会话管理。
- `sync.rs` —— Gist 发现/创建/读写 + 合并引擎。
- HTTP 用 `reqwest`（已有依赖或按需新增），凭据用 `keyring`。

### 4.2 Command 清单（草案）

| Command | 入参 | 返回 | 说明 |
| --- | --- | --- | --- |
| `auth_is_configured` | – | `{ configured, login, avatarUrl }` | 启动时问一次会话状态 |
| `auth_device_start` | – | `{ userCode, verificationUri, expiresIn, interval }` | 发起设备码流程，后台进入轮询 |
| `auth_device_cancel` | – | `void` | 用户取消，终止轮询，不留半态 |
| `auth_logout` | – | `void` | 清凭据库 + 内存 + 广播状态复位 |
| `sync_pull` | – | `{ settings, updatedAt }` | 拉 Gist → 合并 → 应用到本地 |
| `sync_push` | `{ settings }` | `{ ok }` | 前端上报本地变更 → 合并 → 推 Gist |

> 注（2026-10-20 实现裁决）：**`sync_now` 不设独立 Rust 命令**——合并引擎在前端（脏标记在 localStorage），手动「立即同步」由前端编排 pull → 合并 → 有脏键才 push，故 Rust 侧只有 `sync_pull` / `sync_push` 两条命令。

> 命令名与字段名以 Rust serde 序列化输出为准；前端 mock 必须复制真实 Rust 输出（领域铁律，2026-10-18 事故教训）。

### 4.3 事件清单（草案）

| 事件 | 载荷 | 触发 |
| --- | --- | --- |
| `auth:device-success` | `{ login, avatarUrl }` | 轮询拿到 token 落库后 |
| `auth:device-error` | `{ code, message }` | 超时 / 网络穷尽 / 用户拒绝 |
| `auth:session-changed` | `{ configured, login }` | 登录成功或登出（多窗口状态一致） |
| `sync:done` | `{ ok, direction }` | 一轮同步结束（成败均发） |

> 事件名与载荷在稿图阶段随 UI 需求定稿。

### 4.4 登录流程细节

1. `auth_device_start`：请求 device/code，把 `userCode` + `verificationUri` 返回前端展示。
2. 后台任务按 `interval` 轮询 access_token：`authorization_pending` 继续等；`slow_down` 间隔 +5s；`expired_token` 或超出 `expires_in` 终止并广播 `auth:device-error`。
3. 成功：token + 用户信息落凭据库 → 广播 `auth:device-success` → 自动触发一次 `sync_pull`。
4. 网络异常：线性退避重试 8 次（TengriQaghan 实测教训），全败广播 `auth:device-error`。
5. `auth_device_cancel`：AbortHandle 终止轮询任务。

### 4.5 安全教训移植清单（TengriQaghan 踩坑实录 → 本项目规则）

| # | 教训 | 本项目落法 |
| --- | --- | --- |
| 1 | 凭据必须单例加载，防并发读撕 | keyring 读写封装为 Mutex 单例 |
| 2 | 多处同时写凭据会互踩 | 写操作经单通道串行化（writeChain 模式） |
| 3 | 写凭据必须原子化，防半写损坏 | 先写临时值再替换，失败保留旧值 |
| 4 | 切号必须全量清场 | 登出清凭据库（token + gist id）+ 内存会话 + 广播复位 |
| 5 | 「拉取 → 本地应用 → 订阅回调再推」成回环 | `withRemoteApply` 闸门：远端数据落本地时挂闸，本轮回调不上推 |
| 6 | Gist 归属校验缺失导致串号 | §5.3 三元组校验，缺一即拒绝 |

---

## 五、数据设计

### 5.1 Gist 文件结构（phoenix-drx-sync.json）

```json
{
  "schemaVersion": 1,
  "updatedAt": "2026-10-20T12:00:00Z",
  "settings": {
    "soundEnabled": true,
    "motionEnabled": true,
    "locale": "zh-CN",
    "readAnnouncements": ["ann-2026-10", "ann-2026-09"]
  }
}
```

- `settings` 键空间开放：一期 4 键；未来功能加键不改结构，`schemaVersion` 递增做迁移。
- `updatedAt` 仅展示用，合并不依赖它（字段级合并，见 §5.4）。

### 5.2 同步项语义表

| 键 | 类型 | 合并规则 | 备注 |
| --- | --- | --- | --- |
| soundEnabled | bool | last-write-wins | 用户偏好 |
| motionEnabled | bool | last-write-wins | 用户偏好 |
| locale | string | last-write-wins | TengriQaghan 教训：语言属机器偏好（不同设备环境可能偏好不同），倾向留本地——一期先同步，上线看反馈（§十-1） |
| readAnnouncements | string[] | 并集 | 只增不减，天然无冲突 |

> 一期 4 项全是「单值或只增数组」，不存在删除型数据，**tombstone 机制暂不需要**（TengriQaghan 防删除复活用于其预设场景）；§九 纳入预设/收藏时再启用。

### 5.3 归属校验（防串号，TengriQaghan 真实事故）

每次读写 Gist 前校验三元组，缺一即拒绝操作并广播错误：

1. `owner.login` == 当前登录用户名；
2. Gist 文件名 == 约定文件名（`phoenix-drx-sync.json`）；
3. Gist 描述含约定标记（如 `Phoenix-DRX Sync Data`）。

首登发现阶段：只对「同时满足 2 + 3」的 Gist 再校验 1，避免误用他人或旧账号的数据；本机凭据库里已有 gist id 的直接校验全三元组。

### 5.4 合并引擎

- 输入：本地当前值 + 远端 JSON；输出：合并结果 + 是否需要回推。
- 逐键按 §5.2 规则合并。
- last-write-wins 判据用**本地脏标记**（该键有未推送的本地变更 = 本地新）而非时间戳，避免设备间时钟漂移导致误判。
- 无脏标记的键：远端值覆盖本地并应用；有脏标记的键：本地覆盖远端并回推。

---

## 六、前端设计

### 6.1 设置页账号面板

- 入口：设置页新增「GitHub 账号」分区（稿图阶段出图，用户审批后才写生产代码）。
- 未登录：「登录」按钮 → 设备码流程视图：`userCode` 大字展示 + 复制按钮 + 授权链接 + 剩余时间 + 取消。
- 已登录：用户名（头像可选）+ 手动「立即同步」+ 最近同步时间 + 登出。
- 全部自绘件（视觉基线禁用原生件），跟随现有设置页语言与 token。

### 6.2 同步触发时机

| 时机 | 动作 |
| --- | --- |
| 登录成功 | pull 一次 |
| 4 项设置任一变更 | 防抖 1.5s，且距上次推送 ≥ 15s 才推（TengriQaghan 节流教训） |
| 应用启动 | 已配置会话则 pull 一次 |
| 手动「立即同步」 | pull + push 一轮 |
| Gist 请求失败 | 60s 冷却后允许重试；失败不阻塞本地使用 |
| 登出 | 不推，仅清场 |

### 6.3 状态管理与数据流

- 新增 `src/lib/sync.svelte.ts`：会话状态（登录态/账号信息）+ 同步状态（空闲/同步中/最近错误）。
- 现有 4 个模块（sound / motion / i18n / announcements）**保持 localStorage 为第一数据源不变**；sync 模块只做「监听变更 → 上报」与「远端到达 → 应用」，不接管存储（最小侵入，模块自治不破坏）。
- 远端应用统一走 `withRemoteApply` 闸门（§4.5-5）防回环。

### 6.4 UI 文案纪律

平实中文，禁技术黑话（领域铁律）：不说 Gist / OAuth / scope / token，说「GitHub 账号」「云同步」「同步失败，稍后自动重试」。

---

## 七、明确不上云清单

- 存档本体（profile.json）及任何 SPT 数据；
- 存档路径、机器名等环境信息；
- 凭据、代理、自定义 Key 等敏感配置（若未来出现此类设置项，默认留本地）；
- token 本身——只进系统凭据库，永不入 Gist、localStorage、日志。

## 八、验收标准

1. **登录**：设备码展示 → 浏览器授权 → 自动登录成功；取消 / 超时 / 用户拒绝三路均正确收场，无半态残留。
2. **同步**：A 机改音效开关 → B 机（同账号）15 秒内或下次启动时到达；公告已读并集生效；两机同时改同一开关无数据损坏（后操作胜出）。
3. **归属**：换 GitHub 账号登录读不到前账号数据；Gist 被手动改名/篡改内容时校验拦截、不写入。
4. **安全**：token 在凭据库之外（磁盘、日志、localStorage）零出现；登出后凭据库清空。
5. **离线**：断网时登录/同步失败有平实文案提示，本地功能完全不受影响，恢复后自动续推。
6. **门禁**：`npm run gate` 全绿；新增 i18n key 全语种同补；UI 无原生件、无硬编码色值。

前置准备（用户侧）：在 GitHub 新建 OAuth App（勾选 Device Flow），client_id 交付进仓库配置。

## 九、二期展望（依赖未来功能，不放假开关）

| 未来功能 | 数据形态 | 届时同步动作 |
| --- | --- | --- |
| 编辑器预设 | 删除型对象集 | 纳入 settings 扩展键 + 启用 tombstone |
| 收藏方案 | 删除型对象集 | 同上 |
| 对比快照 | 大体量 / 可重建 | 评估只同步元数据，快照体不上云 |

原则：**功能本地先落地，同步随后纳入**；未落地前 UI 不出现对应同步项。

## 十、开放问题（审阅时定夺）

1. **语言（locale）同步还是留本地**：TengriQaghan 教训倾向留本地（机器偏好）；本方案一期先同步，可改。
2. **登录入口位置**：仅设置页内，还是加首次启动引导。稿图阶段定。
3. **client_id 备用双轨**（TengriQaghan 有备用 id）：一期单 id，被限制再加。

## 附：与 TengriQaghan 的技术映射

| TengriQaghan（Electron） | Phoenix-DRX（Tauri 2） |
| --- | --- |
| safeStorage（DPAPI / Keychain） | keyring crate |
| 主进程 fetch | Rust reqwest |
| IPC 通道（authIpc / gistIpc） | Tauri command + event |
| React DeviceFlowModal | Svelte 5 设备码流程视图（待稿图） |
| SyncManager 触发 | sync.svelte.ts 触发 |
| 手机号 + 密码通道（CF Workers） | 一期不做，provider 抽象预留 |
