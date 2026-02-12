# Draft: FastAPI + GraphRAG 智能规则检索集成

## Requirements (confirmed)
- 目标：为现有 AI 代理服务引入微软官方 GraphRAG，实现系统提示词分层（核心规则固定发送 + 自定义规则按需检索）。
- 必须保持核心规则完整性与格式稳定。
- 自定义规则位于指定边界段落之间，需要抽取并入库索引。
- 接口链路：`/chat/completions` 请求拦截、重构、转发。
- 约束：检索失败可降级；中间件延迟目标 <100ms；需要成本优化与缓存策略。
- 交付期望：实现代码、配置文件、文档、验证脚本（后续执行阶段由 Sisyphus 完成）。
- 用户明确要求：当前阶段只做规划，不改代码。
- 用户进一步收敛范围：**不要替换其他链路，仅使用 GraphRAG 做索引能力建设**。

## Technical Decisions (working)
- 当前先进入“深度调研 + 架构访谈”阶段，不直接实施。
- 并行启动子智能体：
  - 代码结构/拦截点/提示词来源分析
  - GraphRAG 官方配置与增量索引研究
  - OSS 集成模式与低延迟模式研究
- 自动化测试策略：YES（Tests-after）。

## Research Findings (interim)
- 仓库内直接 Python FastAPI 代码暂未检出；当前主要命中 Rust/Tauri 代理链路：
  - `src-tauri/src/server/proxy_api.rs`
  - `src-tauri/src/services/llm_client.rs`
  - `src-tauri/src/api/chat.rs`
- 前端有 system prompt 注入字段与消息拼装入口：
  - `src/pages/Index.tsx`
  - `src/pages/Service.tsx`
  - `src/stores/configStore.ts`
- 说明：现阶段可能是“Rust 代理 + 前端注入”，与“FastAPI 中间件”目标存在落差，需要你确认目标部署形态。

## Research Findings (official GraphRAG docs)
- 官方配置核心：`settings.yaml` 的 `completion_models / embedding_models / chunking / input.metadata / vector_store / workflows / update_output_storage`。
- 官方检索 API（库级）支持：`local_search / global_search / drift_search / basic_search` 及 streaming 变体。
- 索引建议：先 `graphrag index --dry-run`，再标准索引；增量可用 `graphrag update` 或 update method。
- 增量输出模式：官方支持 `update_output_storage` + 时间戳目录（便于蓝绿切换与回滚）。
- 本地模型可行但需注意结构化输出稳定性（Ollama/代理链路建议做严格格式校验）。

## Research Findings (OSS + architecture patterns)
- GraphRAG 相关生产信号集中在：超时失败策略、全局检索高延迟、缓存复用与失效边界。
- 可复用模式：
  - 双路径检索（fast path + deep path）
  - 分层缓存（请求级/L1 + 检索中间产物/L2 + 索引构建检查点/L3）
  - 明确 fallback ladder（GraphRAG → local/basic → 缓存 → 核心规则-only）
  - 严格端到端 deadline 与限流（避免尾延迟失控）
- 可选低延迟创意策略（需评估复杂度）：
  - Ghost Session（异步刷新上下文）
  - Holographic Gate（布隆/关键词门控，减少无效检索）
  - Narrative Sharding（先路由后检索）

## Research Findings (repo implementation details)
- 真实聊天主链路（当前代码）：
  - 前端发送：`src/pages/Index.tsx`（`messages` 会把 `system_prompt_injection` 直接放进第一条 system）
  - 后端 API：`src-tauri/src/api/chat.rs`（再次支持注入 `config.system_prompt_injection`）
  - 代理转发：`src-tauri/src/server/proxy_api.rs` + `src-tauri/src/services/llm_client.rs`
- 注入行为关键点：`LlmClient::inject_prompt` 若已有 system 消息会“追加到现有 system 末尾”，否则插入新 system。
- 风险：若未来在多个位置重复注入，可能导致规则重复、顺序漂移、token 不可控。

## Research Findings (request lifecycle & interception)
- 当前仓库**无 FastAPI 运行入口**，后端主实现为 Rust + Axum（Tauri 内嵌服务）。
- 主聊天链路：
  - `src-tauri/src/server/main_api.rs` → `POST /api/chat`
  - `src-tauri/src/api/chat.rs::chat`
  - `src-tauri/src/services/llm_client.rs::chat`
- 代理链路（Ollama 兼容）：
  - `src-tauri/src/server/proxy_api.rs` → `POST /api/chat` / `/api/generate`
  - 预设目标 URL 才是外部 `.../v1/chat/completions`
- 最佳拦截位置（现状下）：
  - `proxy_api.rs` 中构造 `injection` 的位置（chat/generate 两处）
  - 可选同步接入 `api/chat.rs` 以覆盖前端直连聊天路径

## Research Findings (fragility map)
- 规则边界触发有硬编码：`SYSTEM:\n`（前端与后端双处），且与 `fallback_pattern` 配置项存在语义不一致。
- 提示词注入算法是“追加到已有 system 或插入新 system”，对顺序/幂等敏感。
- token 统计当前多为字符长度近似值，不是真实 tokenizer 计数，难直接用于严格 ROI 评估。

## External Guidance (architecture memo synthesis)
- 建议采用：核心规则本地常驻 + 自定义规则异步检索 + 多级缓存 + 熔断降级。
- 关键工程策略：
  - 检索硬超时（如 80ms）
  - fail-open（超时/错误直接 core-only）
  - L1/L2 缓存
  - 规则合并与长度上限
  - 全链路指标（hit ratio、timeout、注入长度、p95 延迟）

## Candidate Architecture Options (working)
- A. 纯 FastAPI 代理：把 `/chat/completions` 拦截、规则分层、GraphRAG 查询都放在 Python 服务。
- B. GraphRAG 独立 sidecar（推荐候选）：保留现有 Rust 代理，只把“规则检索服务”独立成 Python GraphRAG 服务。
- C. 混合：短期 sidecar + 中期逐步迁移 FastAPI 主代理。

## Oracle Guidance (architecture-level)
- 推荐路径：B（GraphRAG sidecar，由现有 Rust 代理调用）
  - 先保留当前 Rust 网关与转发逻辑，最小化迁移风险。
  - 由 Rust 负责最终提示词组装与降级决策（确保核心规则不丢失）。
- 核心不变量（必须满足）：
  - Core Rules 永远存在且优先顺序固定
  - 检索失败不阻塞主链路（除非显式 fail-closed 模式）
  - 检索注入有上限（条数/长度/token）并去重
  - 每请求可审计（规则版本、命中、延迟、降级标记）

## Risks Noted
- 目标描述与仓库现状不一致（FastAPI vs Rust 现网链路）是当前最大阻塞风险。
- GraphRAG 全量查询若直接放请求主路径，可能难满足 <100ms。
- 规则边界 marker 依赖文本锚点，若上游模板变体/换行改变，会导致分割失败。

## Scope Boundaries (draft)
- INCLUDE: 提示词分层策略、GraphRAG 索引策略、请求拦截/重写策略、降级与性能方案、测试验证方案。
- EXCLUDE: 未确认后端技术栈前，不锁定具体落地文件与最终执行步骤。

## Open Questions
- 目标拦截层到底是：
  1) 新建 Python FastAPI 代理层，还是
  2) 在现有 Rust `proxy_api.rs` 中实现等价逻辑？
- 游戏系统提示词来源文件（你提到 423KB JSON）当前是否在本仓库内，还是外部路径读取？
- 你是否要求“必须完全迁移到 FastAPI”，还是“GraphRAG 逻辑可独立服务化，由 Rust/前端调用”？

## Recommended Defaults (pending your confirmation)
- 拓扑默认：B（GraphRAG Python sidecar）
  - 原因：最小入侵现有 Rust 主链路，风险最低，便于逐步迁移。
- 请求期策略默认：
  - Core rules 永远本地拼接
  - Custom rules 走“门控 + 缓存 + 有时限检索（超时即降级）”
  - 检索失败不阻塞主流程，直接 core-only。

## Scope Update (latest)
- IN（当前计划版本）：
  - GraphRAG 官方索引工作区与数据抽取/分块/元数据策略
  - 外部游戏存档 JSON → GraphRAG 输入文档流水线
  - 全量与增量重建策略、成本配置、验证脚本与性能基线
- OUT（当前计划版本）：
  - Rust 代理拦截改造
  - 请求期动态检索注入
  - 任何现有链路替换/迁移

## Latest User Request
- 需要先给出**可视化流程图**（非代码实现），用于直观看到 GraphRAG 如何被使用：
  - 索引构建流程
  - 检索使用流程
- 强调：这是 GraphRAG 使用方案说明，不是实现细节。

## New Context Confirmed by User
- 规则在存档文件中；人物档案在另一路径；对话历史非常长。
- 游戏请求会携带：规则 + 人物档案 + 全量对话历史 + 最新玩家输入。
- 目标流程：
  1) 提前索引规则与人物档案
  2) 用辅助模型快速判断应检索内容
  3) 从索引提取关键内容
  4) 用提取结果替换原始超长内容，再发给高级模型
- 新约束（最新）：
  - 规则与人物档案通常稳定不变。
  - 人物档案字数较小且关键，倾向于每次全量携带（不做检索裁剪）。
  - 规则索引只在用户主动触发时重建（非自动频繁索引）。
- 最大问题转为“动态增长的对话历史如何低延迟处理”。
- 关键新约束：
  - **系统规则（游戏自带）必须全量发送，不可改变。**
  - 可索引部分仅限：自定义规则 + 世界背景。
  - 用户关注：在该约束下是否还能安全缩减，以及 H1 相比全量直发是否更快。

## Latest Question
- 用户询问：检索链路使用轻量模型（如 Gemini Flash），主回答使用高级模型，是否有风险。
- 用户要求：将执行参数速查表同步为“配置驱动版”（不指定具体主/辅模型名）。

## Interim Guidance
- 可行且常见，但需设置风险护栏：
  - 轻量模型只做路由/查询改写，不做最终事实裁决
  - JSON 结构校验 + 置信度门控 + 超时降级
  - 低置信度时提高召回或走保守上下文包
  - 评估指标分开看：路由准确率、召回覆盖率、最终回答正确率

## Sample Request Observation (案例.txt)
- 请求中 `messages[0]` 为超长 system，包含 Mission/Core Rules/Override 大块内容；`messages[1]` 为最新用户输入。
- 当前样例说明：真正的 token 黑洞主要是 system 长规则与潜在长历史（实际线上会继续增长）。

## History Strategy Candidates (for decision)
- H1: 分层记忆（推荐）
  - L0: 最近 N 轮原文（每次带）
  - L1: 会话摘要块（按阈值滚动生成）
  - L2: 长期事实卡（低频入 GraphRAG）
  - 优点：延迟可控、稳定性高、上下文连续性好
- H2: 全量历史先摘要后检索
  - 每次先压缩再查索引
  - 风险：请求时延上升，抖动大
- H3: 仅最近窗口 + 历史全靠 GraphRAG
  - 速度快，但剧情连续性风险较高（召回失败会丢细节）

## User Follow-up
- 用户要求具体解释 H3（最近窗口 + 全靠检索）在实际运行时是什么样。
- 用户偏好确认：希望“失忆风险最低”。
- 用户已确认 H1，并要求提供“案例前后对照表”。

## Decision Direction (proposed)
- 为了最低失忆风险，优先选择 **H1 三层记忆** 或 **H3+状态卡强化版**。
- 在“风险最低”目标下，建议主选 **H1 三层记忆**：
  - L0 最近窗口原文
  - L1 滚动摘要
  - L2 检索记忆
  - 并配合人物档案全量直传、规则低频手动重索引。

## Research Findings (all background tasks complete)

### Repository flow mapping
- 当前仓库主聊天链路（Rust/Axum）为：
  - 前端 `src/pages/Index.tsx` 组装 messages（含历史 + 最新输入）
  - `src-tauri/src/api/chat.rs` 接收并转发
  - `src-tauri/src/services/llm_client.rs` 最终发送到上游模型
- 代理链路：`src-tauri/src/server/proxy_api.rs` 同样会转发消息并注入系统提示。
- 历史压缩模块 `src-tauri/src/services/history_compressor.rs` 读取 `history_compression.save_data_path` 下 JSON（含 `ConversationHistory`）。
- 角色档案来自 `character_configs` 路径（`character_manager.rs`），与 save_data_path 是两套数据系统。

### GraphRAG strategy guidance (official + ultrabrain)
- 官方建议点：
  - 输入文档统一结构 + metadata（artifact_type/persona_id/session_id/version 等）
  - chunking + prepend metadata
  - 按场景选择 query mode：低延迟优先 basic/local，global/drift 做重查询/异步
  - update 工作流支持频繁增量更新
- 检索路由建议：
  - 辅助模型先分类意图（rule-first / profile-first / hybrid）
  - 生成查询模板，再执行 GraphRAG 检索
  - 结果重排 + 冲突处理 + token 预算裁剪
  - 低置信度时按 tiered fallback 退化

### Key risks to reflect in diagrams/plan
- 双注入与链路不一致风险（前端与后端都可能注入 system）。
- save 数据与 character profile 两路径 schema 不一致风险。
- 全量历史替换后可能影响连续性，需要保留最小近期窗口。
- 低延迟目标与召回率之间必须通过路由与预算机制平衡。
