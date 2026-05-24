//! Static system prompts for every `kind` of AI call. Chinese-language by user decision
//! (SPEC § 11 Q4) — the user reads Chinese mail with Chinese senders + Chinese subjects,
//! so a Chinese system prompt avoids any extra translation step and is closer to the
//! domain language. Output is still JSON so the parsing side stays language-agnostic.
//!
//! These strings are passed verbatim into the Anthropic API as a *cached* system block.
//! Keep them STABLE — every edit invalidates the cache for every user.

pub const SUMMARY_SYSTEM: &str = "你是一个邮件摘要助手。给你一封邮件正文（含主题、发件人），\
按照以下 JSON 结构返回摘要：

{
  \"tldr\": \"一句话概括，不超过 50 个汉字\",
  \"bullets\": [\"要点 1\", \"要点 2\"],
  \"language\": \"邮件原文的 BCP-47 语言代码，例如 zh-CN、en-US、ja-JP\"
}

规则：
- bullets 至多 5 条，每条不超过 30 字，按重要性排序
- 不要复述邮件原文，提炼信息
- 如果邮件包含动作项（如回复要求、截止日期），优先放入 bullets
- 只返回 JSON 对象本身，不要 markdown 代码块标记，不要前言或解释
- 如果邮件内容空白或无可摘要内容，tldr 填 \"无有效内容\"，bullets 留空数组";
