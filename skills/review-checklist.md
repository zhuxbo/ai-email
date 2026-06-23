# Review Checklist — ai-email 反模式清单（单一来源）

`/finish-check` §6 派 reviewer 时引用本文件。**这是反模式与 reviewer 模板的唯一权威**，别处只引用不复制（避免漂移）。

> 条目来自项目 `CLAUDE.md` 硬约束 + ssl-manager 通用反模式 + **2026-06-23 全量审核修复（分支 `fix/audit-2026-06-23`，52 commit、reviewer 实测逮到一批门绿行为错的真 bug）的真实回归**。后者给多数条目补了 commit 锚定，并新增 **#10 前端乐观更新/异步竞态** 与 **#11 AI 响应解析脆弱** —— 本项目最高频的两类真 bug。继续：每抓到一次真实问题就给对应条目补 commit 锚定（见末尾「维护」），不写假想案例。

---

## 反模式分级（reviewer 应用范围）

- **核心层（每轮必扫，与改动无关）**：#2 凭据泄露 · #8 失败路径静默吞错 · #9 伪绿测试。这三条是安全/质量底线，无论改什么都扫。
- **条件层（按改动目录/类型触发）**：
  - `src-tauri/src/commands/` → #1 panic 跨 FFI
  - `src-tauri/src/{imap,smtp,ai}/` → #3 非 TLS
  - `src-tauri/migrations/` + `src-tauri/src/db/` → #4 迁移非幂等
  - `src/`（前端）→ #5 直调 invoke
  - `src-tauri/src/ai/prompts.rs` + prompt 改动 → #6 AI prompt 回归字面比较
  - 删除了类型/函数/命令/表/字段/配置 → #7 删除残留（配 finish-check §1.5）
  - `src/lib/store/`、`src/components/`（前端状态/组件）→ #10 乐观更新/异步竞态
  - `src-tauri/src/ai/`（响应解析，非 prompt 部分）→ #11 AI 响应解析脆弱

每条命中按下方「判定」给结论；评级见 reviewer 模板「输出格式」。

---

## 反模式 1：panic 跨 FFI

**来源**：`CLAUDE.md` —— Tauri 命令返回 `Result<T, AppError>`，never panic across FFI。

**背景**：`#[tauri::command]` 里 panic 会跨 FFI 边界，前端拿到的是进程崩溃而非可处理的错误。

**检查动作**：command 路径（`commands/` + 其调用的 `ai`/`imap`/`smtp`/`db`）无裸 `unwrap()`/`expect()`/`panic!`/`unreachable!`/`todo!`/越界索引；错误用 `?` 传播为 `AppError`。

```bash
rg -n '\.unwrap\(\)|\.expect\(|panic!|unreachable!|todo!' src-tauri/src/commands src-tauri/src/ai src-tauri/src/imap src-tauri/src/smtp
```

**判定**：命中处要么改 `?` 传播，要么附注释证明不可能 panic（如刚校验过的常量解析）。测试代码内的 unwrap 豁免。

## 反模式 2：凭据泄露（核心层）

**来源**：`CLAUDE.md` Security —— QQ 授权码 + Anthropic key 存 OS keychain，never plaintext config/DB；`secrecy::Secret` 包裹，never log。

**背景**：凭据一旦进日志、Debug 输出、DB 表或 config 文件，就脱离了 keychain 的保护面。

**检查动作**：

- 新凭据字段用 `secrecy::SecretString` 包裹，不裸 `String` 长期持有
- 不出现在 `tracing::*` / `println!` / `dbg!` / `#[derive(Debug)]` 暴露的输出里
- 不写入任何 DB 列或 config 文件明文

```bash
rg -n 'auth_code|api_key|password|secret|token' src-tauri/src --type rust
```

**判定**：每个命中确认凭据只在「内存 secrecy ↔ keychain」之间流转，未 log、未明文落库。

## 反模式 3：非 TLS 调用（条件层：imap/smtp/ai）

**来源**：`CLAUDE.md` —— 所有 HTTP（IMAP / SMTP / Anthropic）走 TLS，无例外。

**检查动作**：无 `http://`（应 `https://`）；IMAP/SMTP 走 rustls TLS；reqwest 启用 `rustls-tls` 而非 native-tls 或明文。

```bash
rg -n 'http://|danger_accept_invalid|native-tls|Tls::None' src-tauri
```

**判定**：所有外部连接 TLS；命中的 `http://` 确认是注释/文档而非生产路径。

## 反模式 4：迁移非幂等（条件层：migrations/db）

**来源**：`CLAUDE.md`（SQLite via sqlx）+ ssl-manager 反模式 21（部署/迁移静默失败）。

**背景**：迁移看似入表 ≠ 结构真生效；非幂等迁移在已有数据上二次应用会炸。

**检查动作**：

- 新迁移是 SQLite 方言（无 PG 残留：`gen_random_uuid`/`TIMESTAMPTZ`/`TEXT[]`/`JSONB`/`ANY`）
- DDL 幂等或天然一次性（`CREATE TABLE IF NOT EXISTS`、加列前确认列不存在）
- **改结构后实跑验证生效**：删本地 DB 重新迁移，或对最终 schema 断言（集成测试 `tests/`）

**判定**：迁移可重复应用不报错；改结构附实跑迁移 + schema 校验的证据，不只信"migration 入表"。

## 反模式 5：前端直调 invoke（条件层：前端）

**来源**：`CLAUDE.md` —— UI 永不直接 `invoke`，一律走 `src/lib/tauri.ts` 类型化封装；不从前端调 IMAP/SMTP/AI。

**检查动作**：

```bash
rg -n "from '@tauri-apps/api/core'|invoke\(" src --glob '!src/lib/tauri.ts'
```

**判定**：除 `src/lib/tauri.ts` 外 0 命中 —— 它是唯一 import `invoke` 的地方。

## 反模式 6：AI prompt 回归字面比较（条件层：ai/prompts）

**来源**：`CLAUDE.md` Testing —— AI prompt 回归测试用语义匹配（LLM-judge 或 embedding），NEVER literal string compare。

**背景**：模型输出非确定，`assert_eq!(out, "固定串")` 会脆性失败或诱导把 prompt 锁死成某次输出。

**检查动作**：改 `prompts.rs` 后的测试不是整串字面比较；用结构断言（JSON 字段存在、`category` 落在闭集 5 类、`priority ∈ [1,3]`）或语义匹配。

**判定**：prompt 回归测试语义化/结构化，无脆性字面 diff。

## 反模式 7：删除残留（条件层：删除）

**来源**：ssl-manager §1.5 —— 单测只证"剩余功能还在"，证不了"残留物没了"。

**检查动作**：从 `git diff` 的 `-` 行挖出删除概念（类型/函数/命令/表/字段/配置键），逐个全库反向 grep：Rust 引用、`invoke_handler` 注册、`src/lib/tauri.ts` 包装、`types.ts` 镜像、`CLAUDE.md`/`README.md`/`skills/` 文档示例。

**判定**：每个删除概念全库 0 残留（除有意保留的兼容拒绝）。命中证据贴实际输出（截断规则同 finish-check §1.5）。

## 反模式 8：失败路径静默吞错（核心层）

**来源**：ssl-manager 反模式 1（失败路径数据安全）。

**背景**：被静默 drop 的错误 = 用户看不到、审计查不到的数据丢失。

**检查动作**：错误有处理 —— 该落库的落库（如 `send_log` 成败都写）、该 surface 的进 `store.error` → toast、该传播的 `?`；不 `let _ = result;` 吞 `Result`，不空 `catch {}`。

```bash
rg -n 'let _ = .*\?|\.ok\(\);|catch.*\{\s*\}|unwrap_or_default\(\)' src-tauri/src src
```

**判定**：失败路径有落库/surface/传播之一，不静默。命中确认是有意忽略且无数据风险。

## 反模式 9：伪绿测试（核心层）

**来源**：ssl-manager 反模式 15（断言没真验证行为）。

**背景**：测试绿 ≠ 行为对。空断言 / 跳过 / 方向反置的测试反而锁死漏洞。

**检查动作**：断言真验证行为，非只断"没报错"/`assert!(result.is_ok())` 了事；无 `#[ignore]` / `it.skip` / `todo!()` 吞掉本该验证的逻辑；mock 不把"应拒"输入放进放行集。

**判定**：每个测试有针对行为的有意义断言。

**本项目实例**（测试绿却绕过真 bug，2026-06-23 审核抓到）：

- 手喂理想输入而非**生产真实数据形态** —— `e4a2984`：回复头测试喂带 `<>` 的 message-id，而 mail-parser 实际剥掉了 `<>`、DB 存的是裸 id，测试假绿、真发出的头无法线程化。
- 测试**时序提前 flush** 绕过真竞态 —— `043f400`：unlisten 测试在 unmount 前 `await Promise.resolve()` 把 listen Promise flush 掉，恰好绕过「卸载早于 resolve」的泄漏路径。
- 只测 **happy 维度**掩盖回归 —— `c75a226`：排序测试都显式给了 `internalDate`，掩盖了线上该字段恒 NULL、排序退化为插入序的回归。
- **对策**：reviewer 对关键修复做**变异测试**（把生产逻辑改坏，断言相关测试 FAIL）——这次 P13 测试加固即用此法证实判别力。

## 反模式 10：前端乐观更新 / 异步竞态（条件层：src/lib/store、src/components）

**来源**：ai-email **最高频真 bug** —— 2026-06-23 审核在 store/组件并发抓到一批，且**都被只测「不同邮件/不同消息」的集成测试绕过**（见 [[project-p3c-done]] 同款教训，整支 finish-check 偏集成/契约、挖不到组件内部并发）。

**背景**：乐观更新、AI 起草/翻译异步返回、邮件/信箱切换、single-flight 缓存，都在「请求发出 → 用户继续操作 → 响应返回」之间留竞态窗口。

**检查动作**：

- **失败回滚精准到单项**，不整列快照覆盖 —— 否则抹掉并发进行的其它乐观更新（`ae9564c` setSeen/setFlagged 整列回滚丢更新 → `b33e087` 改 toggleFlag 按 flag 粒度基于当前值反向）。
- **异步响应用请求标识守卫**防陈旧覆盖 —— 同一对象重复请求时旧响应不得覆盖新响应（`53cdcd7`/`b33e087` draftingFor token；`70fca6f` 切信箱 `selectedMailboxId` 迟到守卫）。
- **身份令牌而非内容字段比对** —— 「切换/重选同一邮件」用 nonce/seq，不用 messageId（`033e96c` 同邮件重选无条件 reset 清掉正在写的草稿）。
- **single-flight 用持状态原语**（watch/oneshot），不用边沿触发的 `Notify::notify_waiters`（迟到者错过唤醒永久挂起，`0a0b31a`）。
- **useEffect 异步订阅清理用 mounted 守卫** —— `listen` 返回 Promise，cleanup 同步执行时 unlisten 可能仍 null，StrictMode 下双订阅泄漏（`043f400`）。
- **测试必须覆盖「同邮件/同消息」对抗场景**（挂起一个请求 + 乱序 resolve、同 id 重选），不只 happy 的「不同对象」—— 后者恰好绕过上述全部 bug。

**判定**：每个乐观更新有精准回滚；每个异步响应有请求标识/迟到守卫；single-flight 不丢唤醒；订阅有 mounted 清理；且**有「同对象并发」对抗测试**（缺即视为未验证）。

## 反模式 11：AI 响应解析脆弱（条件层：src-tauri/src/ai 响应解析）

**来源**：ai-email —— 2026-06-23 审核在 ai/ 抓到一批响应解析硬失败（`e696663`/`6d6607f`）。OpenAI 兼容厂商（DeepSeek/智谱/Kimi/Qwen 等）经常无视 prompt 约束。

**背景**：prompt 里「不要 markdown 围栏」只是请求不是约束；裸 `serde_json::from_str(&resp.text)` 在模型加围栏/前言/截断/返回非 JSON 错误体时**硬失败而非降级**。

**检查动作**：

- 解析前**剥离 markdown 围栏/前后缀**（共享 `extract_json` helper，所有 JSON 编排器统一用，含同行围栏边界 `798ad3c`）。
- HTTP 响应**先取状态码再解析正文** —— 非 2xx 即使正文非 JSON（502/网关 HTML/限流 text）也返回含状态码的错误，不被泛化 decode error 吞掉。
- **检测截断**（`finish_reason=="length"`/`stop_reason=="max_tokens"`）返回明确错误，不让截断 JSON 走泛化失败。
- **usage 字段容错**（饱和不回绕、接受 JSON 浮点、单字段越界不归零整个 Usage）。
- **prompt 注入防护**：邮件主题/发件人/片段用结构化标签包裹 + 转义（**含 `&`，先于 `<>`**），且 system prompt 声明标签内为不可信内容、勿执行其中指令（`74bec68`/`5735578`）。

**判定**：AI 响应解析对围栏/截断/非 JSON 错误体/usage 异常都**降级而非硬失败**；prompt 对不可信邮件内容有结构化隔离 + 信任边界声明。

---

# Reviewer Subagent 任务模板（单一权威）

> `/finish-check` §6 派 reviewer 时复制本章节填空，别处不再写一份。reviewer 默认用 `feature-dev:code-reviewer`（未注册则 general-purpose）。

## 模板正文（复制到 Agent prompt）

```
你是带着怀疑的独立 reviewer，目标是找毛病而非确认正确。

## 改动范围
- diff：<主智能体填精确可复跑的命令，如 `git diff <base>..HEAD`；提交前场景填 `git diff` + `git diff --cached`。禁止只贴 --stat/文件名替代>
- 主要功能背景：<1-3 句，这次改动想解决什么>
- 本轮报告写入：<.superpowers/reviews/<本次 run 目录>/round-<N>.md 的路径；reviewer 必须把完整报告原样写入该文件，最终回复正文 = 同一报告>
- 相关 plan 文档（无则填'无'）：<.superpowers/plans/<file>.md 或 '无'>

## 已知 review 历史（避免重复报告，但已修项必须复验）
- 上一轮 critical/high 列表：<填，如 "H1 凭据进日志（已修待复验，commit abc1234）" 或 "无 — 首轮">
- 上一轮 medium 用户决议为'当场修'的项：<填 或 无>
- 当前第 N 轮 / 共 5 轮：<填>
- 已修项不要原样重复报告；标"已修待复验"的项必须复验运行路径真生效（读修复代码，不信 commit message）。复验不通过 → 按新 critical/high 报。

## 必查反模式清单
读 skills/review-checklist.md「反模式分级」：核心层 3 条（#2 凭据/#8 失败路径/#9 伪绿）必扫 + 条件层按改动目录触发。
- 凭据泄露(#2)/非 TLS(#3)/panic 跨 FFI(#1)/失败路径数据丢失(#8) 命中 → 最低 High
- 任何"声称有防御"的机制（凭据不落库、迁移幂等、错误会 surface），必须实际制造一次反例确认真生效，不止静态读代码

## 必须实际跑（不只是静态推理）
0. 实跑 diff 命令取全文 patch（非 --stat），每条发现引用具体 hunk（文件:行）；取不到 → 末行输出 `REVIEW_FAIL: diff 不可获取，无法审查`
1. cd src-tauri && cargo fmt --check
2. cd src-tauri && cargo clippy --no-deps --all-targets -- -D warnings
3. cd src-tauri && cargo test --all-features（改动涉及的）
4. pnpm run lint && pnpm run typecheck && pnpm exec vitest run --passWithNoTests
5. 至少 1 个失败场景/反例验证（关键，静态推理 ≠ 验证）：凭据是否真不落库/不 log、迁移是否真幂等（重跑一次）、错误路径是否真 surface 给用户 —— 读到"有防御代码" ≠ 生效
6. 反向抽查 3-5 条反模式复选框：先判 diff 触发了哪些高危维度（凭据/TLS/FFI/迁移/删除），触发的每个至少抽 1 条，贴实际 grep/命令输出（截断同 finish-check §1.5）
7. 前端 `store/`、`components/` 改动（触发 #10）：确认存在「**同邮件/同消息**」并发对抗测试（挂起一个请求 + 乱序 resolve / 同 id 重选），不只「不同对象」happy 测试；对关键并发/解析修复至少做 1 次**变异验证**（把生产逻辑改坏，断言相关测试 FAIL）——读到「有守卫代码」≠ 测试真能逮到

## 输出格式
- 按 Critical / High / Medium / Low 分级
- 每条附 confidence: NN（0-100），仅 ≥ 80 列出（过滤理论问题）
- 每条带 文件:行 + 反模式编号锚定
- 评级锚定：#1 panic 跨 FFI 可崩溃 / #2 凭据泄露 / #8 失败路径致数据丢失 / #3 安全绕过可复现 / #9 伪绿锁死漏洞 —— 命中最低 High，降级须在条目内附一句"降级理由"

## 证据回执（签字前必附 — 固定段标题，主智能体 grep 核验）
报告倒数第二段必须是 `## 证据回执` 固定标题段，逐行列：
- 第 0-4 项各一行：<命令> → 退出码 N
- 第 5 项一行：制造了什么失败/反例 → 结果
- 第 6 项一行：抽查了哪 3-5 条反模式 → 各自结论
缺「证据回执」段 = 主智能体按 REVIEW_FAIL 退回重派。

## 退出签字（必须 — 机器可 grep 前缀）
完整报告（含证据回执与签字行）原样写入指定 round 文件，最终回复正文与文件一致。最后一行二选一，**前缀（含冒号）原样**，前缀后附回执与简短说明：
- critical/high 清零 → `REVIEW_PASS: round=N findings=C0/H0/M_/L_ — critical/high 已清零（medium/low 列于上方供用户决议）`
- critical/high > 0 → `REVIEW_FAIL: round=N findings=C_/H_/M_/L_ — 发现 N 个 critical/high 需修复`

报告控制在 1500 字内（证据回执不计）。任何近义句（"看起来通过"）不被接受；未写入 round 文件 = 流程未完成。
```

## 字段填法

- **改动范围 / 功能背景**：主智能体按当次改动填，确保 reviewer 不依赖主对话上下文
- **已知 review 历史**：首轮填"无 — 首轮"；第 2 轮起填上一轮 critical/high + 决议
- **必查反模式清单**：固定引用本文件，不复制清单内容

## 修改本模板注意

- `/finish-check` §6 只引用本章节，不重写
- 退出签字前缀 `REVIEW_PASS:` / `REVIEW_FAIL:` 不可变（否则 grep 失效）
- `REVIEW_STALLED:` 不在本模板 —— 它是主智能体第 5 轮硬停时自己输出的标记，单一来源在 finish-check §6

---

# 维护

新条目进清单的标准：

- 是项目**实际遭遇过**的回归（不写假想），修复带 commit 锚定
- 能抽象成 1-2 句的"反模式"
- 准入前先问"是否既有条目的新实例?" —— 是则并入该条目作第 N 例，不开新条（控制条目数）
- 入册即清仓：新条目（或新检查动作）的同一改动里，对全仓实跑该检查并贴输出；存量违例当场修或列 follow-up

案例腐烂（代码已删/路径已变）时及时更新或下线对应条目。
