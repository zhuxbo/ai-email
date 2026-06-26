//! 会话感知增量提取（方案 B）。纯函数，无 IO。
//!
//! 核心思想：对方的引用分隔符五花八门、还会 re-wrap，但被引用的文字就是库里上一封正文。
//! 故拿前序正文（prior_bodies）做行/句匹配来定位引用，而不是猜分隔符。
//!
//! 三步管道（判据写死，见设计稿 §3.1）：
//!   1. 剥引用：(a) 经本应用发出且含顶格分隔符 → 锁第一处顶格 marker 切；
//!      (b) 其它来源 → 用紧邻上一封（prior 末项）行匹配 K=2 / 句匹配 M=70%；
//!      (c) 兜底：启发式 On…wrote: / 发件人: / 连续 `>` 块；仍无则不剥。
//!   2. 剥签名：`-- \n` / 尾部联系方式行 / 与 prior 尾部反复出现的相同块 / 命中规则 pattern。
//!   3. 剥重复块：与任一 Some prior 项高度重复的段落。

/// 连续命中阈值：对方引用块归一化后，与上一封连续 K 行一致即判为引用起点。
const QUOTE_LINE_RUN: usize = 2;
/// 按句匹配阈值（re-wrap 场景）：命中句占被检测块的比例 ≥ M% 即判定。
const QUOTE_SENTENCE_RATIO: f64 = 0.70;

// ── 类型（跨 Task 锁定）────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Action {
    #[default]
    Keep,
    Strip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Signature,
    Quote,
    Repeat,
}

#[derive(Debug, Clone, Default)]
pub struct TargetActions {
    pub signature: Action,
    pub quote: Action,
    pub repeat: Action,
    pub signature_pattern: Option<regex::Regex>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedBlock {
    pub kind: Target,
    pub text: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct ExtractResult {
    pub net: String,
    pub removed: Vec<RemovedBlock>,
}

// ── 归一化 helper ──────────────────────────────────────────────────────────────

/// 归一化一行用于跨封比对：去前导引用符 `>`（可叠多层，如 `> > `）、折叠内部连续空白为单空格、trim。
/// 空行归一化为空串。
pub(crate) fn normalize_line(line: &str) -> String {
    let mut s = line.trim_start();
    // 逐层剥 `>`（允许其后跟空格）。
    while let Some(rest) = s.strip_prefix('>') {
        s = rest.trim_start();
    }
    // 折叠内部空白。
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 把整块文本按行归一化，丢弃归一化后为空的行。返回 (原始行号, 归一化文本) 对，保留原始行号以便切回原文。
pub(crate) fn normalized_nonblank_lines(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let n = normalize_line(l);
            if n.is_empty() {
                None
            } else {
                Some((i, n))
            }
        })
        .collect()
}

/// 顶格 marker 正则：行首即 `──────── 原邮件 ────────`（允许尾随空白）。
/// 行首不得有 `>` 或空白 → 内层带 `>` 的嵌套 marker 不命中（拼/剥对称的关键）。
pub(crate) fn top_level_marker_re() -> regex::Regex {
    // 8 个 U+2500 BOX DRAWINGS LIGHT HORIZONTAL，与 smtp 拼接处（Task 5）同一字符串。
    regex::Regex::new(r"^──────── 原邮件 ────────\s*$").expect("marker regex must compile")
}

/// 朴素分句：按中英文句末标点（。！？.!?）与换行切。用于 re-wrap 场景的句级匹配。
/// 去空白后过滤掉空句。
pub(crate) fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if matches!(ch, '。' | '！' | '？' | '.' | '!' | '?' | '\n') {
            let t = cur.split_whitespace().collect::<Vec<_>>().join(" ");
            if !t.is_empty() {
                out.push(t);
            }
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    let t = cur.split_whitespace().collect::<Vec<_>>().join(" ");
    if !t.is_empty() {
        out.push(t);
    }
    out
}

// ── 行匹配 / 句匹配 判据 ────────────────────────────────────────────────────────

/// 在 body 的归一化非空行序列里，找到"连续 ≥K 行都出现在 prior 归一化行集合中"的第一处，
/// 返回该处在 body **原始行号**（切引用从此行起到结尾）。无则 None。
///
/// 仅当连续命中达到 K=QUOTE_LINE_RUN 才算引用起点——避免单行偶合（如一句常见客套）被误判。
pub(crate) fn find_quote_start_by_lines(body: &str, prior: &str) -> Option<usize> {
    let prior_set: std::collections::HashSet<String> = normalized_nonblank_lines(prior)
        .into_iter()
        .map(|(_, t)| t)
        .collect();
    if prior_set.is_empty() {
        return None;
    }
    let body_lines = normalized_nonblank_lines(body);
    let mut run_start: Option<usize> = None; // body_lines 下标
    let mut run_len = 0usize;
    for (idx, (_orig, norm)) in body_lines.iter().enumerate() {
        if prior_set.contains(norm) {
            if run_start.is_none() {
                run_start = Some(idx);
            }
            run_len += 1;
            if run_len >= QUOTE_LINE_RUN {
                // 命中：返回连续段起点的原始行号。
                let start_idx = run_start.unwrap();
                return Some(body_lines[start_idx].0);
            }
        } else {
            run_start = None;
            run_len = 0;
        }
    }
    None
}

/// re-wrap 场景的句级判定：body 末尾被检测块的句子里，有多少比例出现在 prior 句集合中。
/// 返回命中比例（0.0..=1.0）。block 为空 → 0.0。
pub(crate) fn sentence_match_ratio(block: &str, prior: &str) -> f64 {
    let prior_set: std::collections::HashSet<String> = split_sentences(prior).into_iter().collect();
    let block_sentences = split_sentences(block);
    if block_sentences.is_empty() {
        return 0.0;
    }
    let hits = block_sentences
        .iter()
        .filter(|s| prior_set.contains(*s))
        .count();
    hits as f64 / block_sentences.len() as f64
}

/// 句级判定是否过阈值（M=70%）。
pub(crate) fn is_quote_by_sentences(block: &str, prior: &str) -> bool {
    sentence_match_ratio(block, prior) >= QUOTE_SENTENCE_RATIO
}

// ── 句级候选块起点(re-wrap 兜底,不写死中点)──────────────────────────────────

/// 为句级判定挑候选引用块起点:从 body 末尾向上,找命中 prior 行集合的**最长连续后缀**起点
/// (放宽到单行命中,不要求 K=2 —— K=2 已在 find_quote_start_by_lines 先试过且失败)。
/// 返回该后缀首行的 body 原始行号;若末行就不命中(无可锚点)则 None,调用方退中点兜底。
/// 解决「引用起点落在正文前半 → 写死中点漏检/硬切」的缺陷。
fn quote_block_start_for_sentences(body: &str, prior: &str) -> Option<usize> {
    let prior_set: std::collections::HashSet<String> = normalized_nonblank_lines(prior)
        .into_iter()
        .map(|(_, t)| t)
        .collect();
    if prior_set.is_empty() {
        return None;
    }
    let body_lines = normalized_nonblank_lines(body);
    // 从末尾向上,找命中 prior 的最长连续后缀;遇首个不命中行即停。first_hit = 该后缀首行下标。
    let mut first_hit: Option<usize> = None;
    for i in (0..body_lines.len()).rev() {
        if prior_set.contains(&body_lines[i].1) {
            first_hit = Some(i);
        } else {
            break;
        }
    }
    first_hit.map(|i| body_lines[i].0) // 末行就不命中 → None,调用方退中点兜底
}

// ── 启发式引用 marker(兜底)────────────────────────────────────────────────────

/// 兜底引用起点的启发式:常见客户端引用头。命中返回该行原始行号。
fn find_quote_start_heuristic(body: &str) -> Option<usize> {
    let patterns = [
        "wrote:",  // On … wrote:
        "发件人:", // 中文 Outlook
        "发件人：",
        "-----原始邮件-----",
        "------------------ 原始邮件 ------------------", // QQ 邮箱
        "原始邮件",
    ];
    for (i, line) in body.lines().enumerate() {
        let t = line.trim();
        if patterns.iter().any(|p| t.contains(p)) {
            return Some(i);
        }
        // 连续 `>` 引用块:首个以 `>` 开头的行视作引用起点。
        if t.starts_with('>') {
            return Some(i);
        }
    }
    None
}

/// 把原文按行号切成 [0, cut) 保留段。cut 行及之后丢弃。
fn take_lines_before(text: &str, cut: usize) -> String {
    text.lines().take(cut).collect::<Vec<_>>().join("\n")
}

fn lines_from(text: &str, start: usize) -> String {
    text.lines().skip(start).collect::<Vec<_>>().join("\n")
}

// ── 签名剥除 ────────────────────────────────────────────────────────────────────

/// 找签名起点行号。优先级:`-- ` 独立分隔行 > 规则 pattern 命中行 > 尾部联系方式块 >
/// 与 prior 尾部反复出现的相同块(spec §3.1 第 2 步第 3 判据)。返回 None 表示无签名。
/// `prior_last`:紧邻上一封正文(`prior_bodies.last()` 的 text),用于跨封重复签名判定;None 则跳过该判据。
fn find_signature_start(
    body: &str,
    pattern: Option<&regex::Regex>,
    prior_last: Option<&str>,
) -> Option<usize> {
    let lines: Vec<&str> = body.lines().collect();
    // 1) 标准签名分隔符:孤行 `--`(可带尾随空格,RFC 3676 是 `-- `)。
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim_end();
        if t == "--" || t == "-- " {
            return Some(i);
        }
    }
    // 2) 规则 pattern:首个命中行作为签名起点。
    if let Some(re) = pattern {
        for (i, l) in lines.iter().enumerate() {
            if re.is_match(l) {
                return Some(i);
            }
        }
    }
    // 3) 尾部联系方式启发:最后若干行里出现 电话/手机/邮箱/Tel:/Mobile: 样式 → 从该块首行起。
    // ⚠️ 用带边界/冒号的形式,避免裸 "Tel"/"Mobile" 命中正文 "Tell me"/"Mobile-first"。
    let contact_markers = [
        "手机", "电话", "邮箱", "Tel:", "Tel.", "Mobile:", "tel:", "mailto:",
    ];
    let n = lines.len();
    let tail_start = n.saturating_sub(6); // 仅看末 6 行,避免误伤正文。
    for (i, line) in lines.iter().enumerate().skip(tail_start) {
        let t = line.trim();
        if contact_markers.iter().any(|m| t.contains(m)) {
            return Some(i);
        }
    }
    // 4) 跨封重复签名(spec §3.1 第 2 步第 3 判据):body 尾部连续 N≥2 行(归一化后逐行 eq)
    //    与 prior_last 尾部相同 → 那段作签名剥除。对方每封都附同一签名块时命中。
    if let Some(prior) = prior_last {
        if let Some(idx) = find_repeated_tail_start(body, prior) {
            return Some(idx);
        }
    }
    None
}

/// 找 body 尾部与 prior 尾部连续相同的块的起点原始行号。
/// 归一化后从两者末尾向上逐行比对,连续相同行数 ≥2 即认定为跨封重复签名,返回该块在 body 的原始行号。
/// 不足 2 行相同则 None。
fn find_repeated_tail_start(body: &str, prior: &str) -> Option<usize> {
    let body_norm = normalized_nonblank_lines(body); // (原始行号, 归一化文本)
    let prior_norm = normalized_nonblank_lines(prior);
    let mut run = 0usize;
    let (mut bi, mut pi) = (body_norm.len(), prior_norm.len());
    while bi > 0 && pi > 0 {
        if body_norm[bi - 1].1 == prior_norm[pi - 1].1 {
            run += 1;
            bi -= 1;
            pi -= 1;
        } else {
            break;
        }
    }
    if run >= 2 {
        Some(body_norm[bi].0) // bi 现指向连续相同段的第一行
    } else {
        None
    }
}

// ── 重复块剥除 ──────────────────────────────────────────────────────────────────

/// 把文本按空行切成段落(连续非空行为一段)。返回 (起始原始行号, 段落文本)。
fn paragraphs(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut start = 0usize;
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            if !cur.is_empty() {
                out.push((start, cur.join("\n")));
                cur.clear();
            }
        } else {
            if cur.is_empty() {
                start = i;
            }
            cur.push(line);
        }
    }
    if !cur.is_empty() {
        out.push((start, cur.join("\n")));
    }
    out
}

/// 段落与某 prior 是否高度重复:归一化后,段落的非空行有 ≥80% 出现在该 prior 行集合中,
/// 且段落至少 2 行(避免单行巧合误判:宁可漏剥少省 token,不可过剥丢用户正文)。
fn paragraph_is_repeat(para: &str, prior: &str) -> bool {
    let prior_set: std::collections::HashSet<String> = normalized_nonblank_lines(prior)
        .into_iter()
        .map(|(_, t)| t)
        .collect();
    let para_lines = normalized_nonblank_lines(para);
    if para_lines.len() < 2 || prior_set.is_empty() {
        return false;
    }
    let hits = para_lines
        .iter()
        .filter(|(_, t)| prior_set.contains(t))
        .count();
    hits as f64 / para_lines.len() as f64 >= 0.80
}

// ── 主入口 ──────────────────────────────────────────────────────────────────────

/// 把一封邮件正文剥成净增量。三步管道见模块级文档。
///
/// `prior_bodies`:契约见 spec §2.6 —— sent_at 升序,`last()` = 紧邻上一封;第1步行匹配用 `last()`,
/// 第3步重复检测遍历全部 `Some` 项。物化失败/仅 HTML 的前序项为 `None`。
/// `is_own`:当前封是否自己发出(决定第1步走顶格 marker 还是行匹配)。
/// `actions`:每-target 最终动作(规则解析 + 能力默认已在调用方合成)。
pub fn extract_increment(
    body: &str,
    prior_bodies: &[Option<String>],
    is_own: bool,
    actions: &TargetActions,
) -> ExtractResult {
    let mut removed = Vec::new();
    let mut net = body.to_string();

    // ── 第 1 步:剥引用 ──
    if actions.quote == Action::Strip {
        let prior_last = prior_bodies.last().and_then(|o| o.as_deref());
        let mut cut: Option<(usize, String)> = None; // (行号, reason)

        // (a) is_own + 顶格 marker。
        if is_own {
            let re = top_level_marker_re();
            if let Some(idx) = net.lines().position(|l| re.is_match(l)) {
                cut = Some((idx, "经本应用发出,锁顶格分隔符".to_string()));
            }
        }
        // (b) 紧邻上一封行匹配 / 句匹配。
        if cut.is_none() {
            if let Some(prior) = prior_last {
                if let Some(idx) = find_quote_start_by_lines(&net, prior) {
                    cut = Some((idx, "与上一封连续行匹配".to_string()));
                } else {
                    // 句级(re-wrap 兜底):候选引用块起点不写死中点 —— re-wrap 时引用起点可能落在正文前半,
                    // 写死中点会漏检/硬切。改为:从行匹配(放宽到单行命中)的最长命中后缀起点开扫;
                    // 行匹配毫无命中时再退中点兜底。
                    let total = net.lines().count();
                    let start = quote_block_start_for_sentences(&net, prior).unwrap_or(total / 2);
                    let block = lines_from(&net, start);
                    if !block.trim().is_empty() && is_quote_by_sentences(&block, prior) {
                        cut = Some((start, "与上一封按句匹配(re-wrap)".to_string()));
                    }
                }
            }
        }
        // (c) 兜底启发式。
        if cut.is_none() {
            if let Some(idx) = find_quote_start_heuristic(&net) {
                let reason = if prior_last.is_none() {
                    "前序不可用,启发式".to_string()
                } else {
                    "启发式兜底".to_string()
                };
                cut = Some((idx, reason));
            }
        }

        if let Some((idx, reason)) = cut {
            let quoted = lines_from(&net, idx);
            if !quoted.trim().is_empty() {
                removed.push(RemovedBlock {
                    kind: Target::Quote,
                    text: quoted,
                    reason,
                });
                net = take_lines_before(&net, idx);
            }
        }
    }

    // ── 第 2 步:剥签名 ──
    if actions.signature == Action::Strip {
        // 跨封重复签名判据(spec §3.1 第 2 步第 3 条)需紧邻上一封正文;此处独立取(step 1 的同名变量作用域不达此)。
        let prior_last = prior_bodies.last().and_then(|o| o.as_deref());
        if let Some(idx) =
            find_signature_start(&net, actions.signature_pattern.as_ref(), prior_last)
        {
            let sig = lines_from(&net, idx);
            if !sig.trim().is_empty() {
                removed.push(RemovedBlock {
                    kind: Target::Signature,
                    text: sig,
                    reason: "签名分隔符/联系方式/规则命中/跨封重复签名".to_string(),
                });
                net = take_lines_before(&net, idx);
            }
        }
    }

    // ── 第 3 步:剥重复块 ──
    if actions.repeat == Action::Strip {
        let some_priors: Vec<&str> = prior_bodies.iter().filter_map(|o| o.as_deref()).collect();
        if !some_priors.is_empty() {
            // 单遍:命中重复的段落,既记 RemovedBlock,又把该段原始行号区间(段首 start..start+行数)
            // 收进 strip_set。不再二次 filter 重算 paragraph_is_repeat、不留只用于判空的 strip_starts。
            let mut strip_set: std::collections::HashSet<usize> = std::collections::HashSet::new();
            for (start, para) in paragraphs(&net) {
                if some_priors.iter().any(|p| paragraph_is_repeat(&para, p)) {
                    strip_set.extend(start..start + para.lines().count());
                    removed.push(RemovedBlock {
                        kind: Target::Repeat,
                        text: para,
                        reason: "与历史邮件段落高度重复".to_string(),
                    });
                }
            }
            if !strip_set.is_empty() {
                net = net
                    .lines()
                    .enumerate()
                    .filter(|(i, _)| !strip_set.contains(i))
                    .map(|(_, l)| l)
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
    }

    ExtractResult {
        net: net.trim_end().to_string(),
        removed,
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn normalize_line_strips_quote_markers_and_folds_space() {
        assert_eq!(normalize_line("> > 你好   世界"), "你好 世界");
        assert_eq!(normalize_line("   plain  text "), "plain text");
        assert_eq!(normalize_line(">"), "");
        assert_eq!(normalize_line("   "), "");
    }

    #[test]
    fn normalized_nonblank_drops_empty_keeps_orig_lineno() {
        let text = "a\n\n> b\n  \nc";
        let got = normalized_nonblank_lines(text);
        // 行号：0='a', 2='b', 4='c'（1、3 归一化后为空被丢）。
        assert_eq!(got, vec![(0, "a".into()), (2, "b".into()), (4, "c".into())]);
    }

    #[test]
    fn top_level_marker_matches_only_at_column_zero() {
        let re = top_level_marker_re();
        assert!(re.is_match("──────── 原邮件 ────────"));
        assert!(re.is_match("──────── 原邮件 ────────   ")); // 尾随空白允许
                                                             // 内层带 `>` 前缀 → 不命中（嵌套 marker）。
        assert!(!re.is_match("> ──────── 原邮件 ────────"));
        // 行首带空白 → 不命中。
        assert!(!re.is_match("  ──────── 原邮件 ────────"));
    }

    #[test]
    fn split_sentences_handles_cjk_and_ascii() {
        let s = split_sentences("你好。世界！ How are you? Fine.");
        assert_eq!(s, vec!["你好", "世界", "How are you", "Fine"]);
    }

    #[test]
    fn find_quote_start_needs_two_consecutive_lines() {
        let prior = "下周有空聊聊合作吗\n我在北京";
        // body：一行净增量 + 两行引用（连续命中 prior 两行）。
        let body = "好的没问题\n下周有空聊聊合作吗\n我在北京";
        // 第 1 行（原始行号 1）起进入连续命中 → 返回 1。
        assert_eq!(find_quote_start_by_lines(body, prior), Some(1));
    }

    #[test]
    fn find_quote_start_single_line_coincidence_not_matched() {
        let prior = "下周有空聊聊合作吗\n我在北京";
        // body 只有一行偶合 prior（单行，未达 K=2）→ 不剥。
        let body = "好的\n下周有空聊聊合作吗\n另说一件别的事";
        assert_eq!(find_quote_start_by_lines(body, prior), None);
    }

    #[test]
    fn find_quote_start_none_when_prior_empty() {
        assert_eq!(find_quote_start_by_lines("any\nthing", ""), None);
    }

    #[test]
    fn sentence_ratio_over_threshold_for_rewrapped_quote() {
        let prior = "这周五开会方便吗。地点在三楼会议室。请准时。";
        // 对方客户端把上面三句 re-wrap 成不同换行，但句子内容一致。
        let block = "这周五开会方便吗。\n地点在三楼会议室。\n请准时。";
        assert!(is_quote_by_sentences(block, prior));
        assert!((sentence_match_ratio(block, prior) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn sentence_ratio_below_threshold_for_fresh_text() {
        let prior = "这周五开会方便吗。地点在三楼会议室。";
        let block = "周五我不行。改下周一可以吗。我再确认下日历。";
        assert!(!is_quote_by_sentences(block, prior));
    }
}

#[cfg(test)]
mod pipeline_tests {
    use super::*;

    fn defaults_strip_all() -> TargetActions {
        TargetActions {
            signature: Action::Strip,
            quote: Action::Strip,
            repeat: Action::Strip,
            signature_pattern: None,
        }
    }

    // ── 第 1 步 剥引用 ────────────────────────────────────────────────────────

    #[test]
    fn strips_quote_via_top_level_marker_when_own() {
        // is_own + 顶格 marker → 锁第一处顶格切。
        let body = "好的,周五下午两点。\n\n──────── 原邮件 ────────\n发件人: 张三\n这周五开会方便吗?\n> ──────── 原邮件 ────────\n> > 更早的内容";
        let prior = vec![Some("这周五开会方便吗?".to_string())];
        let r = extract_increment(body, &prior, true, &defaults_strip_all());
        assert_eq!(r.net.trim(), "好的,周五下午两点。");
        assert!(r.removed.iter().any(|b| b.kind == Target::Quote));
        // 内层带 `>` 的 marker 不当顶格 → 整个历史块被一次切掉,而非切到内层。
        assert!(!r.net.contains("更早的内容"));
    }

    #[test]
    fn strips_quote_via_prior_line_match_for_peer() {
        // 对方来信、无我们的分隔符 → 用紧邻上一封行匹配(连续 ≥2 行)。
        let body = "收到,我安排一下。\n下周有空聊聊合作吗\n我在北京等你";
        let prior = vec![Some("下周有空聊聊合作吗\n我在北京等你".to_string())];
        let r = extract_increment(body, &prior, false, &defaults_strip_all());
        assert_eq!(r.net.trim(), "收到,我安排一下。");
        assert!(r.removed.iter().any(|b| b.kind == Target::Quote));
    }

    #[test]
    fn strips_rewrapped_quote_by_sentences() {
        // re-wrap:对方把上一封换行打乱,但句子内容一致 → 句级 ≥70% 命中。
        let prior = vec![Some(
            "这周五开会方便吗。地点在三楼会议室。请准时。".to_string(),
        )];
        let body = "周五可以的。\n\n这周五开会方便吗。\n地点在三楼会议室。\n请准时。";
        let r = extract_increment(body, &prior, false, &defaults_strip_all());
        assert!(r.net.contains("周五可以的"));
        assert!(
            !r.net.contains("三楼会议室"),
            "re-wrap 引用应被剥:\n{}",
            r.net
        );
    }

    #[test]
    fn strips_quote_starting_in_first_half() {
        // 引用起点落在正文前半(净增量仅 1 行、引用 4 行 → 起点 line 1,而非中点 line 2)也要完整剥。
        let prior = vec![Some(
            "历史第一行\n历史第二行\n历史第三行\n历史第四行".to_string(),
        )];
        let body = "好的收到。\n历史第一行\n历史第二行\n历史第三行\n历史第四行";
        let r = extract_increment(body, &prior, false, &defaults_strip_all());
        assert_eq!(
            r.net.trim(),
            "好的收到。",
            "前半起点的引用应整段剥净(含首行):\n{}",
            r.net
        );
        assert!(r.removed.iter().any(|b| b.kind == Target::Quote));
    }

    #[test]
    fn heuristic_fallback_when_prior_none() {
        // prior 末项为 None(物化失败/仅 HTML)→ 走启发式 On…wrote:,reason 标注。
        let body = "Thanks, will do.\nOn Mon, Jun 25, 2026 at 10:30 AM Zhang <z@x.com> wrote:\n> old content here";
        let prior = vec![None];
        let r = extract_increment(body, &prior, false, &defaults_strip_all());
        assert!(r.net.contains("Thanks, will do."));
        assert!(!r.net.contains("old content here"));
        assert!(
            r.removed
                .iter()
                .any(|b| b.kind == Target::Quote && b.reason.contains("启发式")),
            "兜底剥引用 reason 应含「启发式」"
        );
    }

    #[test]
    fn single_message_skips_line_match_but_still_strips_signature() {
        // 单封/首封会话:prior 空 → 跳过第1步行匹配与第3步,仅签名 + 启发式。
        let body = "你好,这是第一封。\n\n-- \n张三\n手机:138xxxx";
        let prior: Vec<Option<String>> = vec![];
        let r = extract_increment(body, &prior, false, &defaults_strip_all());
        assert!(r.net.contains("这是第一封"));
        assert!(!r.net.contains("138xxxx"), "签名应被剥");
        assert!(r.removed.iter().any(|b| b.kind == Target::Signature));
    }

    // ── 第 2 步 剥签名 ────────────────────────────────────────────────────────

    #[test]
    fn strips_signature_delimiter() {
        let body = "正文内容。\n-- \nBest regards\nAlice\nalice@example.com";
        let prior: Vec<Option<String>> = vec![];
        let r = extract_increment(body, &prior, false, &defaults_strip_all());
        assert_eq!(r.net.trim(), "正文内容。");
        assert!(r.removed.iter().any(|b| b.kind == Target::Signature));
    }

    #[test]
    fn keeps_legitimate_double_dash_in_prose() {
        // 负例:正文里合法的 `--`(非签名分隔符,后面不是 ` \n` 也非孤行)不应被剥。
        let body = "方案 A -- 成本低但慢;方案 B -- 快但贵。我倾向 A。";
        let prior: Vec<Option<String>> = vec![];
        let r = extract_increment(body, &prior, false, &defaults_strip_all());
        assert_eq!(r.net.trim(), body, "正文内 `--` 不是签名,应原样保留");
        assert!(!r.removed.iter().any(|b| b.kind == Target::Signature));
    }

    #[test]
    fn strips_signature_by_rule_pattern() {
        // 规则 pattern 命中 → 剥签名(即使无 `-- ` 分隔符)。
        let body = "正文。\n本邮件含免责声明 Disclaimer: 内容仅供参考。";
        let prior: Vec<Option<String>> = vec![];
        let actions = TargetActions {
            signature: Action::Strip,
            quote: Action::Strip,
            repeat: Action::Strip,
            signature_pattern: Some(regex::Regex::new("Disclaimer").unwrap()),
        };
        let r = extract_increment(body, &prior, false, &actions);
        assert!(!r.net.contains("Disclaimer"), "命中 pattern 的签名行应被剥");
    }

    #[test]
    fn signature_kept_when_action_keep() {
        // translate 默认 sig=keep → 签名保留。
        let body = "正文。\n-- \nAlice";
        let prior: Vec<Option<String>> = vec![];
        let actions = TargetActions {
            signature: Action::Keep,
            quote: Action::Strip,
            repeat: Action::Strip,
            signature_pattern: None,
        };
        let r = extract_increment(body, &prior, false, &actions);
        assert!(r.net.contains("Alice"), "sig=keep 时签名保留");
    }

    #[test]
    fn contact_marker_substring_not_false_positive() {
        // 负例(收紧 contact_markers 边界):末 6 行出现 "Tell me ... Mobile design" —— 含 "Tel"/"Mobile"
        // 子串但非联系方式,不应被当签名剥。收紧为 "Tel:"/"Mobile:" 等带边界形式后此例保留。
        let body =
            "项目进展同步。\n下一步计划如下。\nTell me more about Mobile design.\n我们周五细聊。";
        let prior: Vec<Option<String>> = vec![];
        let r = extract_increment(body, &prior, false, &defaults_strip_all());
        assert_eq!(
            r.net.trim(),
            body,
            "Tell/Mobile 子串非联系方式,不剥:\n{}",
            r.net
        );
        assert!(!r.removed.iter().any(|b| b.kind == Target::Signature));
    }

    // 仅签名步打开(quote/repeat=Keep)——隔离测第 4 判据,避免签名行同时被 quote 行匹配抢先剥成 Quote。
    fn sig_only_strip() -> TargetActions {
        TargetActions {
            signature: Action::Strip,
            quote: Action::Keep,
            repeat: Action::Keep,
            signature_pattern: None,
        }
    }

    #[test]
    fn strips_cross_message_repeated_signature_block() {
        // 第 4 判据(spec §3.1 第 2 步第 3 条):body 尾部连续 3 行与 prior 尾部相同 → 作签名剥。
        // 无 `-- ` 分隔符、无 contact marker、无规则 pattern —— 仅靠跨封重复命中。
        // quote=Keep 隔离:否则这几行也命中 prior 行集合、会被第 1 步当引用先剥(那样 kind 就成 Quote)。
        let sig = "张三\n产品部\n某某科技有限公司";
        let body = format!("这次说点新内容。\n关于排期我们再确认一下。\n{sig}");
        let prior = vec![Some(format!("上一封的正文不同。\n但落款一样。\n{sig}"))];
        let r = extract_increment(&body, &prior, false, &sig_only_strip());
        assert!(r.net.contains("说点新内容"));
        assert!(
            !r.net.contains("某某科技"),
            "跨封重复签名块应被剥:\n{}",
            r.net
        );
        assert!(
            r.removed.iter().any(|b| b.kind == Target::Signature),
            "应记一条 Signature 被剥块"
        );
    }

    #[test]
    fn keeps_tail_when_differs_from_prior_signature() {
        // 负例:body 尾部与 prior 尾部不同 → 第 4 判据不命中,不剥(无其它签名特征)。
        let body = "全新内容一。\n全新内容二。\n这是本封独有的结尾。";
        let prior = vec![Some("旧内容。\n完全不同的旧结尾。".to_string())];
        let r = extract_increment(body, &prior, false, &sig_only_strip());
        assert_eq!(
            r.net.trim(),
            body,
            "尾部与 prior 不同,不应剥签名:\n{}",
            r.net
        );
        assert!(!r.removed.iter().any(|b| b.kind == Target::Signature));
    }

    // ── 第 3 步 剥重复块 ──────────────────────────────────────────────────────

    #[test]
    fn strips_repeated_block_matching_any_prior() {
        // 与某个(非紧邻)prior 项高度重复的段落 → 剥。
        let body = "新进展:已签约。\n\n关于项目背景的说明如下,\n这是很长的一段重复历史描述内容。";
        let prior = vec![
            Some("关于项目背景的说明如下,\n这是很长的一段重复历史描述内容。".to_string()),
            Some("完全不相关的上一封内容。".to_string()),
        ];
        let r = extract_increment(body, &prior, false, &defaults_strip_all());
        assert!(r.net.contains("已签约"));
        assert!(
            !r.net.contains("项目背景的说明"),
            "与任一 prior 重复的块应被剥"
        );
        assert!(r.removed.iter().any(|b| b.kind == Target::Repeat));
    }

    #[test]
    fn keeps_legitimate_repeat_when_all_prior_none() {
        // 全 None → 跳过第3步,即使正文内有重复也不剥(无可比对基准)。
        let body = "确认一下:周五两点。确认一下:周五两点。";
        let prior: Vec<Option<String>> = vec![None, None];
        let r = extract_increment(body, &prior, false, &defaults_strip_all());
        assert_eq!(r.net.trim(), body, "全 None 时第3步跳过");
        assert!(!r.removed.iter().any(|b| b.kind == Target::Repeat));
    }

    #[test]
    fn repeat_kept_when_action_keep() {
        // repeat=Keep 时第3步跳过。quote=Keep 隔离:否则第1步句级匹配会把与 prior 高度相同的段落当引用剥。
        let body = "新内容。\n\n关于项目背景的说明如下,这是很长的一段重复历史描述内容。";
        let prior = vec![Some(
            "关于项目背景的说明如下,这是很长的一段重复历史描述内容。".to_string(),
        )];
        let actions = TargetActions {
            signature: Action::Strip,
            quote: Action::Keep,
            repeat: Action::Keep,
            signature_pattern: None,
        };
        let r = extract_increment(body, &prior, false, &actions);
        assert!(r.net.contains("项目背景的说明"), "repeat=keep 时保留");
    }
}
