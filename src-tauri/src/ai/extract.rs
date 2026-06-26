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
// Task 4 extract_increment 接入后移除
#[allow(dead_code)]
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
// Task 4 extract_increment 接入后移除
#[allow(dead_code)]
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
// Task 4 extract_increment 接入后移除
#[allow(dead_code)]
pub(crate) fn top_level_marker_re() -> regex::Regex {
    // 8 个 U+2500 BOX DRAWINGS LIGHT HORIZONTAL，与 smtp 拼接处（Task 5）同一字符串。
    regex::Regex::new(r"^──────── 原邮件 ────────\s*$").expect("marker regex must compile")
}

/// 朴素分句：按中英文句末标点（。！？.!?）与换行切。用于 re-wrap 场景的句级匹配。
/// 去空白后过滤掉空句。
// Task 4 extract_increment 接入后移除
#[allow(dead_code)]
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
// Task 4 extract_increment 接入后移除
#[allow(dead_code)]
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
// Task 4 extract_increment 接入后移除
#[allow(dead_code)]
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
// Task 4 extract_increment 接入后移除
#[allow(dead_code)]
pub(crate) fn is_quote_by_sentences(block: &str, prior: &str) -> bool {
    sentence_match_ratio(block, prior) >= QUOTE_SENTENCE_RATIO
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
