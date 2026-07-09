# 完成检查 — ai-email

提交前逐项检查。复用现有 lefthook/hooks 的质量门，再叠加它们没有的三件事：**范围审查、删除审核、独立 reviewer 循环**。"跳过不涉及"以 §1 范围表为准，不允许主智能体自由裁量。

> 技术栈：Tauri 2（Rust）+ React/TS + 嵌入式 SQLite。Rust 命令在 `src-tauri/` 目录下跑。

**S 档降级（仅纯文档/配置改动）**：`git diff HEAD --name-only` + `git diff --cached --name-only` 合并后 `grep -vE '\.(md|ya?ml|toml|json)$'` 为空 → 跳过 §2 的代码测试项（保留对改动 md 跑 prettier）、§3 代码残留扫描；§6 降为 1 轮，reviewer「必须实际跑」第 1-4 项免除、改为核对文档与代码一致性。其余维持全量。

---

## 1. 确定变更范围

```bash
git status --short
git diff --stat
git diff --cached --stat
```

对照下表逐项判「是/否」，决定后面重点跑什么。**不确定一律视为「是」**。

| 维度                                            | 涉及？   | 触发                                                        |
| ----------------------------------------------- | -------- | ----------------------------------------------------------- |
| `src-tauri/src/commands/`                       | 是/否    | #1 panic 跨 FFI 必查；命令返回 `Result<T, AppError>`        |
| `src-tauri/src/imap/`、`smtp/`                  | 是/否    | #3 TLS；集成测试用 in-memory，**绝不碰真 QQ**               |
| `src-tauri/src/ai/`                             | 是/否    | #3 TLS；token 成本；#11 响应解析降级；改 prompt 见下一行    |
| `src-tauri/src/ai/prompts.rs`                   | 是/否    | #6 prompt 改动用语义/结构断言，非字面比较                   |
| `src-tauri/migrations/` + `src/db/`             | 是/否    | #4 迁移幂等 + SQLite 方言；改结构实跑迁移验证生效           |
| 凭据面（secrecy/keyring/auth_code/api_key）     | 是/否    | #2 凭据泄露核心查                                           |
| `src/`（前端）                                  | 是/否    | #5 不直调 invoke；prettier/eslint/tsc/vitest                |
| `src/lib/store/`、`src/components/`（前端并发） | 是/否    | #10 乐观更新精准回滚 / 异步竞态守卫 /「同对象并发」对抗测试 |
| 删除了类型/函数/命令/表/字段/配置               | 是/否    | §1.5 删除审核必跑                                           |
| `.github/workflows`、`lefthook.yml` 等门禁配置  | 是/否    | 改后确认无 job/检查被无意短路或禁用                         |
| **兜底**：上述未覆盖的改动路径                  | 文件清单 | 逐个声明归入上面哪行，或写一句「确认无触发」，不留空        |

**棘轮规则**：判「是」的行要降级须附一行理由；不确定只能升不能降。

---

## 1.5 删除审核（仅当删除了类型/函数/命令/表/字段/配置）

单测只证「剩余功能还在」，证不了「残留物没了」。

1. 从 `git diff` / `git diff --cached` 的 `-` 行挖出删除概念（类型名 / 函数 / 命令 / 表 / 字段 / 配置键）
2. 每个概念全库反向 grep，命中即清：

   ```bash
   rg -n '已删类型名'
   rg -n '已删命令' -- '*.rs' '*.ts' '*.md'
   ```

3. 重点查：`invoke_handler` 注册、`src/lib/tauri.ts` 包装、`types.ts` 镜像、Model 字段引用、`CLAUDE.md`/`README.md`/`skills/` 文档示例
4. **证据格式**：每个被 grep 概念贴实际输出 —— 0 命中贴 `` `<cmd>` → 0 命中 `` 一行；1-5 命中贴全；>5 贴前 5 行 + `... 共 N`

**判定**：全部 0 命中，或命中只剩有意保留项（兼容拒绝/工具链文件）。

---

## 2. 质量门（机器可验证，必跑）

照抄即可（与 lefthook 一致）。逐项贴退出码或末行输出。

```bash
# Rust（在 src-tauri/ 下）
cd src-tauri && cargo fmt --check
cd src-tauri && cargo clippy --no-deps --all-targets -- -D warnings
cd src-tauri && cargo test --all-features

# 前端（项目根）
pnpm run format:check                    # prettier --check .
pnpm run lint                            # eslint --max-warnings 0
pnpm run typecheck                       # tsc --noEmit
pnpm exec vitest run --passWithNoTests

# 密钥
gitleaks protect --staged --no-banner --redact -v

# 依赖变更时（Cargo.toml / package.json 动了；cargo audit 平时在 CI）
cd src-tauri && cargo audit --ignore RUSTSEC-2023-0071
```

`RUSTSEC-2023-0071` 仅来自 `Cargo.lock` 中 `sqlx-macros` 可选 MySQL 后端路径的 `rsa`，本项目 `sqlx` 已禁用默认特性并只启用 SQLite，运行/编译路径不使用 MySQL/RSA。任一不过当场修根因，**禁止 `--no-verify` 绕过**。IMAP/AI 集成测试一律 in-memory/mock，**绝不打真 QQ、绝不真烧 token**。

---

## 3. Git Diff 审查

```bash
git diff
git diff --cached
git status --short | grep '^??'
```

- [ ] 无 `println!`/`dbg!`/`eprintln!` 调试残留（生产用 `tracing`）；前端无 `console.log`/`debugger`
- [ ] 无硬编码密钥/URL/token；`.env` 未误提交
- [ ] 删除的代码直接删，不注释保留
- [ ] `Cargo.lock` / `pnpm-lock.yaml` 无意外变更
- [ ] 无未使用的 `use`（Rust）/ `import`（TS）
- [ ] 未跟踪文件（`??`）：测试临时产物清理；被生产代码引用的 `git add`

---

## 4. 文档同步

- [ ] 核心逻辑改动 → `CLAUDE.md`
- [ ] 用户面功能 → `README.md`（仅用户层关心的特性，内部机制不写）
- [ ] 模块 / 操作手册 → `skills/*.md`
- [ ] 不引用 `.superpowers/` 下过程文档（不入库，引用即悬空链接）

---

## 5. 已知局限性与风险

按分类列出：

**安全**：新命令是否暴露未授权操作；凭据是否只在 secrecy ↔ keychain 间流转、未 log/未明文落库；HTML 邮件正文是否经 DOMPurify 清洗（去 script/on\*/javascript:）后注入 Shadow DOM，远程图片按分类策略（仅 personal/work 默认放行，其余拦截到 data-blocked-src）；AI 调用是否把邮件内容发往非预期端点。

**数据**：迁移是否丢数据 / 能否在线平滑执行；`send_log` 审计行只增不删；缓存键（`prompt_hash`）是否含必要区分量（target/intent），防碰撞串味。

**兼容**：Rust serde 形状改动是否同步 `types.ts`；命令签名变更是否影响 `tauri.ts` 封装与前端调用；跨层数据**真实形态**一致（如 message-id 经 mail-parser 剥 `<>`、`internal_date` 线上恒 NULL —— 别信单元假设，追生产真实值）。

**并发/竞态（#10）**：前端乐观更新失败是否精准回滚单项（非整列快照）；异步响应（AI 起草/翻译、邮件/信箱切换）是否有请求标识 / 迟到守卫防陈旧覆盖；single-flight 是否用持状态原语不丢唤醒；useEffect 订阅是否 mounted 守卫清理（StrictMode 双订阅）；后端 async 是否有 TOCTOU、后台 spawn 是否可取消；是否有「同对象并发」对抗测试（仅测「不同对象」会绕过）。

**性能/成本**：AI 调用 token 上限（正文截断）；批量分类 ≤20/请求；大邮箱查询走索引。

**平台**：改动是否在 macOS(arm64) + Android(arm64-v8a) 都成立；keychain 在 Android 走 android-keyring。

---

## 6. 独立 Review 循环（必跑，不可跳过）

**目的**：用干净上下文的独立 reviewer subagent 以「破坏模式」找毛病，避开主智能体「改完测试过就停手」的偏差。

**质量门定义**：声明「完成」前，**落盘的 round 文件**中必须存在以 `REVIEW_PASS:` 为前缀的签字行（`grep -F` 验证前缀）。签字必须有产物 —— grep 的对象是 `.superpowers/reviews/` 下的落盘文件，不是主智能体自己上下文里的转述。

### 6.1 执行结构（循环到收敛，最多 5 轮）

```
进入前：建本次专属目录 .superpowers/reviews/<时间>-<主题>/
loop:
  ① 派 reviewer subagent（见 6.2），prompt 里「本轮报告写入」填该目录 round-<N>.md
  ② reviewer 返回后：确认报告已落盘 round-<N>.md（缺失或与返回正文不一致 → 本轮无效重派）；
     实跑 grep -Fn "REVIEW_PASS:" 或 "REVIEW_FAIL:" <round 文件>，并确认含 "## 证据回执" 段
  ③ 主智能体读报告：
     ├─ Critical/High：必须修 → 修完跳回 §2（只对改动文件）→ 重新执行 §6；生效由下一轮 reviewer 复验
     ├─ Medium：报告给用户决议（当场修 / follow-up / 接受），主智能体不擅自处理；
     │          用户选「当场修」视同 High（修完重新 §6，并写入下一轮 prompt 的已决议字段）
     └─ Low/Nit：默认 follow-up，不阻塞
  ④ round 文件 grep 命中 REVIEW_PASS: → 退出循环
  ⑤ 第 5 轮仍有 critical/high → 主智能体停止，输出 REVIEW_STALLED: 交用户决定
```

**强制约束**：

- 声明完成前**必须对落盘 round 文件实跑 `grep -Fn "REVIEW_PASS:"`**，并在总结中引用「文件路径 + 命中行」。无落盘文件或无命中 → 流程未完成。
- 轮次数 = 本次 run 目录内 `round-*.md` 文件数（可数文件防虚报，不用整个 reviews/ 目录计数）。
- reviewer 输出 `REVIEW_FAIL:` → 按「修 critical/high → 重跑 §6」处理，不允许只见 PASS 缺失就推断「继续循环」。
- Medium 不允许主智能体擅自修或忽略，必须列给用户决议。
- 第 5 轮硬停，输出 `REVIEW_STALLED: 已达 5 轮上限，等待用户决策`，并逐轮引用第 1-5 轮 round 文件各自的 `REVIEW_FAIL:` 签字行（堵「第 1 轮就喊 STALLED 把球踢给用户」）。

### 6.2 Reviewer 调用

用 Agent tool 派独立子对话，`subagent_type: "feature-dev:code-reviewer"`（未注册则用 general-purpose）。

**Prompt 模板单一来源**：`skills/review-checklist.md`「Reviewer Subagent 任务模板」章节 —— 本文件不内嵌，主智能体派 reviewer 前读该章节，按字段填空（改动范围 / 已知 review 历史 / plan 路径）。最终引用 `REVIEW_PASS` 签字行处注明本轮实际用的 subagent_type。

### 6.3 终止防御

- confidence ≥ 80 才报告（过滤理论问题）。
- 5 轮硬上限；每轮传「已知问题 + 决议」给下一轮，标「已修待复验」项下一轮必须复验运行路径生效才真正豁免。
- 三类机器标记（全程 `grep -F` 验证前缀，不凭语义近义句）：`REVIEW_PASS:`（reviewer 通过签字）/ `REVIEW_FAIL:`（reviewer 失败签字）/ `REVIEW_STALLED:`（主智能体第 5 轮硬停自输出，reviewer 不输出）。

---

逐项检查完毕、§6 落盘 round 文件被 `grep -F "REVIEW_PASS:"` 命中并在总结中引用「文件路径 + 命中行」后，输出结果摘要 + 风险列表，**等用户确认「提交」再执行 git commit**（main 分支不自动提交）。
