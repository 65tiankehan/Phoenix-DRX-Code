# Phoenix-DRX（phoenix-drx）项目准则

> 由白板 `AGENTS.template.md` 实例化（2026-10-16）。
> 标注【通用】的区块不改只追加；标注【领域】的区块由本项目长出。

---

## 〇、项目边界（最优先）【领域】

**主项目就是 `e:\WorldModelGeneraor\Phoenix-DRX`（SPT 存档编辑器：Phoenix 暗黑沉浸外壳 + 参考 4.1.1 功能全集；SPT 版本基准已裁决升 4.1.x（2026-10-20），数据层按 4.1.1 参考校准）。**

工作区里还挂着别的仓库，它们的规则文件**与本项目无关，不要读、不要遵守、不要引用**：

- `e:\WorldModelGeneraor\Phoenix-master` —— 旧版 Phoenix（UI 供体）。只读参考：提炼视觉 token 与布局语言。其激活码/VIP 系统已裁决移除，**禁止捡回**；不许改动它的任何文件。
- `e:\WorldModelGeneraor\SPT-AKI-Profile-Editor-4.1.1` —— SkiTles55 参考编辑器（功能基准，2026-10-20 由 4.0.1 升级；4.0.1 旧仓保留只读可对照）。只读参考：功能全集、SPT 数据结构（Models/Core）、问题诊断规则的对照标准；不许改动。
- `D:\TengriQaghan\Standard Project Goal` —— 项目开张白板（流程标准供体）。只在立规/开新基线文档时读模板；不参与日常开发。
- `e:\WorldModelGeneraor\SPT-AKI-Profile-Editor-4.1.1\CHREADME.md` —— 参考仓库自述，同上只读。

与本项目相关的文档只有以下几份：

- [AGENTS.md](AGENTS.md) —— 本章程（规则源头）
- [docs/guides/交接-新对话必读.md](docs/guides/交接-新对话必读.md) —— 交接主入口（当前态速览 + 卷链）
- [docs/gates/门禁清单.md](docs/gates/门禁清单.md) —— 硬标准台账 + 事故登记
- [docs/design/视觉基线.md](docs/design/视觉基线.md) —— token 单一来源 + 原生件替换表（与稿图同等效力）
- [docs/design/感官层基线.md](docs/design/感官层基线.md) —— 音效/动效/动态图标纪律
- `docs/prototypes/` —— 稿图目录（单文件 HTML，用户审批后才写生产代码）

## 一、通用协作纪律【通用 · 不许删改，只许追加】

### 1. 大文件必须分次写，别撞输出上限

**判据**：预计单次 `Write` 的内容超过约 300 行 / 8000 token，就不许一次写完。

正确做法：

1. 先 `Write` 骨架 + 第一个模块
2. 再用编辑工具逐块追加剩余内容，一次一块
3. 每次追加前不要输出分析性文字，把输出预算留给文件本身

禁止：写大文件的同一回复里先输出大段分析；被截断后原样重试（必须换分次写）。

### 2. 输出前先估体量

动笔前一句话问自己：这次要吐多少字？单文件大组件/长文档 → 分次；多个中等文件 → 一次一个；只改几行 → 直接编辑。

### 3. 省上下文

- 读文件先想清楚看哪一段，能用 `offset` / `limit` 就不整文拉取；超过 100KB 的文档必须先 Grep 定位再分段读。
- 长输出命令在命令里过滤，不把原始输出全灌进来。
- 大批量搜索交给子代理，让它只回摘要。
- 截图一次只取一张，看完即判断。

### 4. 同一文件的连续编辑必须串行

并行发出针对同一文件的多次编辑，第二次会基于旧内容快照执行，把第一次的改动覆盖掉。不同文件可以并行，同一文件必须一次一条、等结果再发下一条。

### 5. 不主动 commit、不主动 push

除非用户在当前对话里明确说了，否则不执行 `git commit` / `git push`。

### 6. 新增枚举值先全仓穷举

新增枚举值前，先全仓 Grep 穷举所有映射表 / switch / 记录点，一次补齐，不许只改定义处。

### 7. 多会话并行时按文件归属分批提交

提交前先盘点归属，只提交本会话的文件；公共文件被两边都改过时，用 `git diff` 拆 hunk 分家，绝不把别人未完成的工作裹进自己的 commit。

### 8. Windows 环境事实：python3 是 Store 占位 stub

本机 `python3` 指向 Microsoft Store 占位程序。仓库内脚本不得依赖 `python3`；确需 Python 时用 `python` 并在脚本头注明。

## 二、诚实原则：不放假开关【通用】

UI 上出现的任何开关、按钮、数据，**必须有真实消费链路**。

- 做不到就不要画。设计稿里也一样，画了就意味着承诺能实现。
- 能力已经落地却还挂「即将开放」角标，属于文档欠账，应当清账。
- 不确定能否实现的元素，必须在交付时**显式标注为待确认**，让用户拍板。

## 三、门禁基线【领域 · 从第一天就填】

- 门禁基线：`npm run gate`（= `check` svelte-check 0 错 0 警 + `lint` eslint + `test` vitest run）全绿（2026-10-17 随技术栈迁移 Tauri 2 + Svelte 5 换轨实测；**以最新交接卷的数字为准，别抄本行旧值**；旧 typecheck:node/typecheck:web 口径作废）。
- **i18n 多语强一致是硬门禁**：zh-CN / en-US key 集合完全相等，由 vitest 卡死；加任何 key 必须全语种同补。
- **视觉 token 单一来源是硬门禁**：`src/app.css` 必须包含视觉基线声明的全部 `--phx-*` token，由 vitest 卡死；组件与页面禁止硬编码色值。
- 原生件禁用（`window.confirm/alert/prompt`、`new Audio`、原生 `title` 等）：eslint `no-restricted-syntax` 卡点**已生效**（`eslint.config.mjs` 6 条，2026-10-17 落地，详见门禁清单）。

## 四、领域铁律【领域 · 空白起步，随事故生长】

> 格式五件套：**规则 + 判据 + 正确做法 + 禁止 + 已发生的事故**。每次翻车后在此追加一条，并在门禁清单事故登记表记一行。

- **存档写入前必须先自动备份**。判据：任何写入 profile JSON 的 IPC 通道，执行前必须先落备份文件（沿旧 Phoenix BackupService 模式）。正确做法：主进程统一封装 `backupBeforeWrite`。禁止：裸写 profile。事故：暂无（数据安全默认，先立后守）。
- **SPT 数据结构以参考项目 Model / 真实服务器 database 为准，不许凭记忆写。** 判据：类型定义须对应 `SPT-AKI-Profile-Editor-4.1.1` 的 ProfileClasses 或真实 profile/database 样本。禁止：拍脑袋造字段。事故：暂无。
- **免费开源，无激活码/VIP/有效期系统。** 判据：仓库中不得出现 ActivationService、有效期徽章、VIP 文案。事故：旧 Phoenix 存在此系统，2026-10-16 裁决移除。
- **面向用户的文案用平实中文，禁止技术黑话**（F30、JSON、IPC 等字眼不得出现在 UI）。判据：review 对照。事故：暂无。
- **稿图上的每个元素都必须落地**（见 §五）。禁止：凭空删元素、自行替换实现。事故：暂无。
- **透明弹窗（独立小窗）禁用玻璃拟态，一律实心 token 底**。判据：凡独立 WebviewWindow（托盘面板、气泡等 transparent 窗口），面板底色必须是实心 `--phx-*` 色值。正确做法：实心底 + CSS 圆角 + `--phx-shadow-win`（托盘面板用 `--phx-surface-2`）；`backdrop-filter` 只能糊到同窗口内的内容，够不到窗外桌面。禁止：透明窗口里用 `--phx-glass` + `backdrop-filter`。事故：2026-10-18 托盘面板玻璃底透出桌面（见门禁清单事故登记）。
- **前端字段名以 Rust serde 序列化输出为准；mock 必须来自真实 Rust 输出。** 判据：新增/修改 IPC 数据结构时，TS 类型与页面绑定的字段名逐一对照 src-tauri 结构体（含 `#[serde(rename_all)]` 后的实际 JSON key）；mock/测试数据从真实命令返回复制。禁止：按前端想象造字段名、用臆造 mock 验证集成。事故：2026-10-18 设置页绑定 `autoCompleteMasterings` 而实际是 `autoAddMasterings`，读到 undefined 后 Svelte 5 `bind:checked={undefined}` 硬错误炸掉整页渲染（真窗口正文空白，见门禁清单事故登记）。

## 五、设计稿先行与视觉基线【领域 · 前端项目适用】

流程固定为：**文档 → 稿图 → 视觉基线 → 生产代码**，跳步即违规。

- 有稿图的屏，稿图上的每个元素都必须落地，不许凭空删、不许自行替换。
- 设计稿放 `docs/prototypes/`，单文件独立 HTML，内联样式，严格复用项目 token（`--phx-*` 前缀，单一来源，不许在稿图里另立一套值）。
- **视觉基线与感官层**已定稿：[视觉基线.md](docs/design/视觉基线.md) 与 [感官层基线.md](docs/design/感官层基线.md)，写任何 UI 组件前先读。
- **禁用原生视觉件**：`window.confirm/alert/prompt`、原生 `title` tooltip、原生 select/checkbox/滚动条/选区等一律用项目自绘替代（完整映射见视觉基线清单）；系统级例外（文件选择、托盘、权限弹窗）白名单明示。
- **光效模式**：本项目为暗色沉浸美学，一期**不启用**光效模式库；未来启用须先在视觉基线声明启用清单。
- 实现时**见到原生件要主动标注**并提议替代方案，而不是等用户指出来。
- **用户确认后才动生产代码。**
