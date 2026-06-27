/// HTML → plain-text conversion (no extra deps; uses the project-bundled `regex` crate).
///
/// Pipeline:
/// 1. Strip `<script>` / `<style>` blocks including their content.
/// 2. Convert block-level closing tags and `<br>` variants to newlines.
/// 3. Strip all remaining HTML tags.
/// 4. Decode common HTML entities (`&amp;` last to avoid double-decoding).
/// 5. Collapse 3+ consecutive blank lines → 2; trim trailing whitespace per line.
pub fn html_to_text(html: &str) -> String {
    use regex::Regex;

    // 1. Remove <script …>…</script> and <style …>…</style> (DOTALL + case-insensitive).
    let re_blocks =
        Regex::new(r"(?is)<(script|style)[^>]*>.*?</(script|style)>").expect("valid regex");
    let s = re_blocks.replace_all(html, "");

    // 2. Block-level closers + <br> variants → newline.
    //    Covers </p> </div> </tr> </li> </h1>…</h6> and <br> <br/> <br />.
    let re_block = Regex::new(r"(?i)<br\s*/?>|</(p|div|tr|li|h[1-6])>").expect("valid regex");
    let s = re_block.replace_all(&s, "\n");

    // 3. Strip all remaining tags.
    let re_tags = Regex::new(r"<[^>]+>").expect("valid regex");
    let s = re_tags.replace_all(&s, "");

    // 4. Decode HTML entities.
    //    Order matters: decode &amp; LAST so that e.g. "&amp;lt;" becomes "&lt;" not "<".
    let s = s
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", "\u{a0}")
        .replace("&amp;", "&");

    // 5. Trim trailing whitespace per line; collapse 3+ consecutive blank lines to 2.
    let trimmed: Vec<&str> = s.lines().map(|l| l.trim_end()).collect();
    let mut result = String::with_capacity(s.len());
    let mut blank_run = 0usize;
    for line in &trimmed {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 2 {
                result.push('\n');
            }
        } else {
            blank_run = 0;
            result.push_str(line);
            result.push('\n');
        }
    }

    result.trim_matches('\n').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_keeps_linebreaks() {
        let html = "<html><head><style>x{}</style></head><body><p>第一段</p><p>第二段</p>\
                    <div>行A<br>行B</div><script>evil()</script></body></html>";
        let t = html_to_text(html);
        assert!(!t.contains('<'));
        assert!(!t.to_lowercase().contains("evil")); // script 内容去除
        assert!(!t.contains("x{}")); // style 内容去除
        assert!(t.contains("第一段") && t.contains("第二段"));
        assert!(t.contains("行A") && t.contains("行B"));
        let lines: Vec<&str> = t.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(lines.len() >= 3); // 块级元素产生换行
    }

    #[test]
    fn decodes_common_entities() {
        assert_eq!(
            html_to_text("a&amp;b&lt;c&gt;d&nbsp;e")
                .replace('\u{a0}', " ")
                .trim(),
            "a&b<c>d e"
        );
    }
}
