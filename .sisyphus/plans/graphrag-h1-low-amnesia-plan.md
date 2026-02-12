# GraphRAG H1 + 双模型方案（低失忆优先，基于案例请求）

## TL;DR

> **Quick Summary**: 采用 H1 三层记忆（L0 最近窗口 + L1 滚动摘要 + L2 GraphRAG 检索记忆）+ 双模型分工（辅助模型负责路由/检索规划，主模型负责最终生成）。模型选择不硬编码：主模型使用现有 API 配置，辅助模型使用首页已有“辅助模型自动切换”配置。
>
> **Deliverables**:
> - 规则/人物/历史的分层数据规范
> - GraphRAG 索引与增量更新策略（规则手动、历史自动）
> - 双模型请求期检索路由策略（辅助模型 + 高级模型）
> - 案例请求的处理方案预览（替换前后）
> - 全套验证与性能/风险检查
>
> **Estimated Effort**: Large
> **Parallel Execution**: YES - 4 waves
> **Critical Path**: 数据分层规范 → 索引资产构建 → 检索路由 → 请求组包与验证

---

## Context

### 用户已确认事实
- 规则在存档文件中，变动低频。
- 人物档案（背景、性格）短且关键：每次全量上传。
- 历史最长且持续增长：必须重点优化。
- 用户确认目标：**失忆风险最低**。
- 用户确认策略：**H1 三层记忆**。
- 用户确认技术方向：**双模型**（轻量模型用于检索链路，高级模型用于最终回复）。
- 刚性约束：**系统规则（游戏自带）必须全量发送，不可改写、不缩减。**

### 代码链路观察（当前）
- 前端会拼装消息并包含历史：`src/pages/Index.tsx`
- 后端接收并转发：`src-tauri/src/api/chat.rs`
- 最终上游请求构造：`src-tauri/src/services/llm_client.rs`
- 历史压缩现有能力：`src-tauri/src/services/history_compressor.rs`

---

## Work Objectives

### Core Objective
在不牺牲角色一致性与剧情连续性的前提下，将“全量历史直发”改造为“分层记忆 + 按需检索”，把历史 token 占用从线性增长改为受控增长。

### Must Have
- H1 三层记忆生效：L0/L1/L2 明确边界与职责。
- 人设全量直传不裁剪。
- 系统规则全量直传且逐字不变。
- 规则索引仅手动触发重建。
- 历史检索失败时仍保持可用与连续（L0+L1兜底）。
- 双模型职责隔离：轻量模型不负责最终答案。

### Must NOT Have
- 不允许纯 H3（仅窗口+检索）导致高失忆风险。
- 不允许每次重建规则索引。
- 不允许改写或缩减游戏自带系统规则。
- 不允许将全量历史继续无上限塞入最终请求。

---

## H1 三层记忆设计（最终选型）

### L0（短期原文层）
- 内容：最近 N 轮对话原文（建议 6~10 轮）
- 作用：保留即时语境、语气衔接、代词指代
- 特性：每次请求固定携带

### L1（滚动摘要层）
- 内容：旧历史按批次压缩后的会话摘要块
- 作用：保留中程剧情连续性（事件、关系变化、未完成事项）
- 更新：达到阈值后增量生成，不每轮重算

### L2（检索记忆层 / GraphRAG）
- 内容：长期事实卡、关键承诺、关系状态、世界设定、规则片段
- 作用：按需召回远期细节
- 特性：查询触发时才注入，非每轮注入

---

## 索引与更新策略

### 系统规则（不可变区）
- 策略：每次请求全量发送，保持原始文本和顺序。
- 管控：保存 `rule_set_hash`，发送前后校验一致性。
- 说明：此区不进“可裁剪池”，不参与摘要与检索替换。

## 1) 规则索引（低频）
- 触发：仅用户点击“重建规则索引”时执行
- 输入：**仅自定义规则 + 世界背景**分块 + 元数据（rule_type、priority、keywords、source、version）
- 更新：手动全量/增量（以版本号和更新时间管理）

## 2) 人物档案
- 不做检索裁剪，始终全量携带
- 仅在档案变更时更新档案索引（可选）

## 3) 历史索引（高频）
- 对新增历史做增量处理：
  - 先进入 L0
  - 超阈值历史进入 L1 摘要
  - 摘要抽取出的长期事实进入 L2
- 避免每次全量重建历史索引

---

## 请求期路由与组包策略

### 双模型职责
- **辅助模型（来自现有辅助模型配置）**：
  - 只做 intent 分类、查询改写、检索规划、置信度评估
  - 输出严格 JSON（固定 schema）
- **主模型（来自现有 API 配置）**：
  - 只做最终回复生成
  - 不承担检索规划职责

### 模型来源（配置驱动，不写死）
- 主模型：沿用当前 `api_url/api_key/model_name` 配置。
- 辅助模型：沿用首页已有“辅助模型自动切换”配置（`fallback_model_enabled/fallback_preset_id/preset_configs`）。
- 要求：检索链路读取现有配置，不新增“写死模型名”分支。

### 辅助模型职责（快速路由，Model A）
- 输入：最新用户输入 + L0 + L1 摘要头部
- 输出：
  - 是否需要 L2 检索
  - 需要的检索类型（规则/关系/事件/人物记忆）
  - 检索查询模板选择
  - 置信度 `confidence`

### 路由 JSON 约束（必须）
- 字段：`intent`, `need_retrieval`, `query_plan[]`, `confidence`, `risk_flags[]`
- 失败处理：JSON 解析失败/字段缺失即走保守降级（不阻塞）

### GraphRAG 检索模式（低延迟优先）
- 默认：`local/basic`（具体以实际 GraphRAG 版本能力为准）
- 升级：当置信度不足时升级到更广检索（如 global/drift）
- 兜底：检索失败则只用 L0 + L1 + 人设全量

### 检索阈值（建议默认）
- 最近窗口：6~10轮 或 1200~1800 tokens（先到先止）
- local 超时：350ms；global 超时：700ms
- local: topK 24 → rerank后8
- global: topK 40 → rerank后12
- 低置信度阈值：`confidence < 0.62` 直接降级；`0.62~0.72` 走扩召回

### 降级链（确定版）
1. 正常：L0 + L1 + L2命中 + 人设全量 + 系统规则全量
2. 弱命中：扩大一次召回并限制注入长度
3. 超时/失败：L0 + L1 + 人设全量 + 系统规则全量（不带 L2）
4. 严重异常：仅最小可用包（系统规则全量 + 人设全量 + L0）

### 组包顺序（最终请求）
1. **系统规则全量（不可变）**
2. 人物档案全量（固定）
3. L1 摘要（关键段）
4. L2 命中片段（去重裁剪后）
5. L0 最近窗口
6. 最新用户输入

---

## 案例处理方案预览（基于 `案例.txt`）

## 原始样例特征
- `messages[0]` 是超长 system（Mission/Core Rules/Override 大块）
- `messages[1]` 是最新玩家输入

## 处理预览（目标态，不是代码）

### Step A：拆分
- 从超长 system 中抽取：
  - 系统规则全量（必须原样保留）
  - 可索引块：自定义规则 + 世界背景（入规则索引）

### Step B：固定保留
- 人物档案（背景+性格）全量保留

### Step C：历史分层
- 最近 6~10 轮原文进入 L0
- 更老的历史由 L1 摘要承载
- 关键长期事实通过 L2 检索回填

### Step D：替换组包
- 保持“系统规则全量”不变；
- 用“人设全量 + L1 + L2命中 + L0 + 最新输入”替换原来的“历史全量直发和可索引规则全量直发”。

### Step E：发送高级模型
- 高级模型接收到的是“短而全”的结构化上下文，而不是历史洪水

---

## Execution Strategy

### Wave 1：规范与基线
- [ ] 1. 定义 L0/L1/L2 数据边界与 schema
- [ ] 2. 定义规则/人物/历史三类文档元数据标准
- [ ] 3. 建立 baseline（当前全量直发的 token/延迟/质量）

### Wave 2：索引体系
- [ ] 4. GraphRAG workspace 与 settings 落地
- [ ] 5. 规则手动重索引流程
- [ ] 6. 历史增量索引流程（L1→L2）

### Wave 3：请求期决策与组包
- [ ] 7. 辅助模型路由策略（触发条件、查询模板、阈值）
- [ ] 8. 检索结果重排、去重、预算裁剪策略
- [ ] 9. 最终上下文组包与降级策略
- [ ] 10. 双模型护栏（JSON校验、置信度门控、超时控制、回退）

### Wave 4：验证与收敛
- [ ] 11. 功能验证（不失忆场景回放）
- [ ] 12. 性能验证（延迟、吞吐）
- [ ] 13. 成本验证（token 节省与索引成本）
- [ ] 14. 双模型一致性验证（路由正确率、JSON成功率、安全一致性）

---

## Acceptance Criteria（可执行）

- [ ] AC1: 长对话回放中，关键关系/承诺在 20+ 轮后仍可被正确引用
- [ ] AC2: 检索失败时输出仍连续（L0+L1 兜底，不出现明显“失忆跳变”）
- [ ] AC3: 人设一致性稳定（语气、身份、背景不漂移）
- [ ] AC4: 相比全量直发，平均请求 token 明显下降
- [ ] AC5: 规则索引仅在手动触发时更新，非手动不重建
- [ ] AC6: 系统规则在请求前后 hash 一致（零改写）
- [ ] AC7: 路由 JSON 解析成功率达到目标阈值
- [ ] AC8: 双模型链路不增加明显失忆率

### Agent-Executed QA Scenarios（示例）

```
Scenario: 长线剧情记忆回放
  Tool: Bash
  Steps:
    1. 用固定剧本连续生成 30 轮对话
    2. 第31轮提问“你还记得之前承诺过什么吗？”
    3. 断言答案命中已记录承诺事项
  Evidence: .sisyphus/evidence/h1-memory-recall.txt

Scenario: 检索失败降级稳定性
  Tool: Bash
  Steps:
    1. 人为禁用 GraphRAG 查询
    2. 发送依赖历史的提问
    3. 断言系统仍返回可用连续回答（不崩溃）
  Evidence: .sisyphus/evidence/h1-degrade-stability.txt

Scenario: 规则手动重索引控制
  Tool: Bash
  Steps:
    1. 正常多轮会话不触发重索引
    2. 断言索引版本号未变化
    3. 用户触发“重建规则索引”后版本号变化
  Evidence: .sisyphus/evidence/h1-rule-reindex-control.txt

Scenario: 双模型路由 JSON 稳定性
  Tool: Bash
  Steps:
    1. 连续发送多种意图样本给轻量模型路由器
    2. 断言输出均可按 schema 解析
    3. 统计解析失败率
  Evidence: .sisyphus/evidence/h1-dualmodel-json-stability.txt

Scenario: 轻量模型低置信度降级
  Tool: Bash
  Steps:
    1. 构造歧义请求触发低置信度
    2. 断言系统执行降级链（不使用不可靠 L2）
    3. 断言最终响应仍连续可用
  Evidence: .sisyphus/evidence/h1-dualmodel-lowconf-fallback.txt
```

---

## Risk Register

1. **摘要失真**：L1 若压缩过度会丢细节
   - 缓解：关键事实抽取并写入 L2；摘要模板保留关系/承诺字段

2. **召回偏差**：L2 检索不稳定
   - 缓解：辅助模型置信度门控 + 多路查询模板 + rerank

3. **上下文顺序混乱**：组包顺序错误造成模型行为漂移
   - 缓解：固定组包顺序与校验器

4. **规则更新滞后**：手动索引导致短期不一致
   - 缓解：UI 显示当前规则索引版本与更新时间

5. **双模型语义漂移**：轻量模型路由与高级模型生成目标不一致
   - 缓解：固定 schema + 置信度门控 + 低置信度升级/降级

6. **JSON结构不稳定**：轻量模型偶发格式错误
   - 缓解：严格 schema 校验、失败即回退默认检索计划

---

## Success Criteria

- [ ] 失忆风险显著降低（长线回放稳定）
- [ ] 人设一致性显著提升（不因裁剪而漂移）
- [ ] token 消耗显著下降且可解释
- [ ] 规则索引维护成本可控（手动触发）

---

## 附录A：执行参数速查表（配置驱动版）

> 说明：本附录不指定具体模型名称。主模型与辅助模型均从现有配置读取。

### A1. 模型来源（仅配置，不写死）

| 参数键 | 来源 | 用途 |
|---|---|---|
| `api_url` / `api_key` / `model_name` | 现有主配置 | 主模型最终生成 |
| `fallback_model_enabled` | 首页辅助模型开关 | 是否启用辅助模型链路 |
| `fallback_preset_id` | 首页辅助模型配置 | 选择辅助模型预设 |
| `preset_configs` | 现有预设配置 | 辅助模型 API/Key/Model 读取 |

### A2. 双模型职责参数

| 参数 | 建议值 | 说明 |
|---|---:|---|
| `router_allow_final_answer` | `false` | 辅助模型禁止输出最终答案 |
| `router_output_schema_required` | `true` | 路由输出必须符合 JSON Schema |
| `router_max_retries` | `1` | 路由 JSON 失败后最大重试次数 |

### A3. 系统规则与人设硬约束

| 参数 | 建议值 | 说明 |
|---|---:|---|
| `system_rules_mutable` | `false` | 系统规则不可改写 |
| `system_rules_send_mode` | `full_exact` | 系统规则每次全量发送 |
| `system_rules_hash_check` | `true` | 发送前后 hash 一致校验 |
| `persona_send_mode` | `full_exact` | 人设每次全量发送 |

### A4. H1 三层记忆参数

| 参数 | 建议值 | 说明 |
|---|---:|---|
| `l0_recent_turns` | `8` | 最近窗口轮数（6~10 可调） |
| `l0_recent_token_cap` | `1500` | L0 token 上限 |
| `l1_summary_enabled` | `true` | 启用滚动摘要 |
| `l1_update_every_turns` | `20` | 每新增 20 轮更新 L1 |
| `l1_summary_token_cap` | `700` | L1 注入上限 |
| `l2_retrieval_enabled` | `true` | 启用 L2 检索 |
| `l2_max_snippets` | `8` | L2 片段上限 |

### A5. 检索阈值与超时

| 参数 | 建议值 | 说明 |
|---|---:|---|
| `router_conf_high` | `0.72` | 高置信度阈值 |
| `router_conf_low` | `0.62` | 低置信度阈值 |
| `retrieval_mode_default` | `local` | 默认检索模式 |
| `retrieval_mode_fallback` | `global` | 弱命中升级模式 |
| `local_topk_pre` | `24` | local 初筛候选 |
| `local_topk_post` | `8` | local 重排后保留 |
| `global_topk_pre` | `40` | global 初筛候选 |
| `global_topk_post` | `12` | global 重排后保留 |
| `router_timeout_ms` | `300` | 路由超时 |
| `retrieval_local_timeout_ms` | `350` | local 超时 |
| `retrieval_global_timeout_ms` | `700` | global 超时 |
| `rerank_timeout_ms` | `250` | 重排超时 |

### A6. 降级链参数

| 参数 | 建议值 | 说明 |
|---|---:|---|
| `degrade_on_timeout` | `true` | 任一关键阶段超时即降级 |
| `degrade_path_level1` | `L0+L1+persona+system` | 不带 L2 的稳态降级 |
| `degrade_path_level2` | `L0+persona+system` | 严重异常最小可用包 |

### A7. 预算配额（总输入预算 = B）

| 区块 | 比例建议 |
|---|---:|
| 系统规则（固定） | 30~40% |
| 人设全量（固定） | 10~15% |
| L0 最近窗口 | 15~20% |
| L1 摘要 | 8~12% |
| L2 检索片段 | 12~18% |
| 余量缓冲 | 8~10% |

### A8. 上线门槛（Go/No-Go）

| 指标 | 建议门槛 |
|---|---|
| 路由 JSON 解析成功率 | ≥ 99.5% |
| 关键记忆召回率（回放集） | ≥ 90% |
| 系统规则 hash 一致率 | 100% |
| p95 额外处理延迟 | ≤ 800ms |
| 失忆率（相对基线） | 不上升 |
