use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use regex::Regex;
use std::sync::LazyLock;

/// 自动模式识别高亮规则
struct HighlightRule {
    regex: Regex,
    style: Style,
}

static RULES: LazyLock<Vec<HighlightRule>> = LazyLock::new(|| {
    vec![
        // HTTP 状态码 2xx (绿)
        HighlightRule {
            regex: Regex::new(r"\b[2]\d{2}\b").unwrap(),
            style: Style::default().fg(Color::Green),
        },
        // HTTP 状态码 4xx (黄)
        HighlightRule {
            regex: Regex::new(r"\b[4]\d{2}\b").unwrap(),
            style: Style::default().fg(Color::Yellow),
        },
        // HTTP 状态码 5xx (红)
        HighlightRule {
            regex: Regex::new(r"\b[5]\d{2}\b").unwrap(),
            style: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        },
        // 耗时超过 1000ms（红色加粗）
        HighlightRule {
            regex: Regex::new(r"\((\d{4,})ms\)").unwrap(),
            style: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        },
        // 耗时 (xxxms) 正常
        HighlightRule {
            regex: Regex::new(r"\(\d+ms\)").unwrap(),
            style: Style::default().fg(Color::Cyan),
        },
        // 错误关键词
        HighlightRule {
            regex: Regex::new(r"(?i)\b(error|exception|failed|failure|crash)\b").unwrap(),
            style: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        },
        // URL
        HighlightRule {
            regex: Regex::new(r"https?://\S+").unwrap(),
            style: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::UNDERLINED),
        },
        // 请求/响应方向箭头
        HighlightRule {
            regex: Regex::new(r"[→←]|->|<-").unwrap(),
            style: Style::default()
                .fg(Color::Rgb(245, 169, 127)) // Peach
                .add_modifier(Modifier::BOLD),
        },
        // HTTP 方法
        HighlightRule {
            regex: Regex::new(r"\b(GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS)\b").unwrap(),
            style: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        },
        // API 路径
        HighlightRule {
            regex: Regex::new(r"/[\w\-]+(?:/[\w\-.]+)+").unwrap(),
            style: Style::default()
                .fg(Color::Rgb(165, 173, 206)), // Subtext0
        },
    ]
});

/// 对文本应用自动高亮规则，返回着色后的 Spans
/// base_style 是没有匹配时使用的默认样式（包含正确的 bg 色）
pub fn auto_highlight(text: &str, base_style: Style) -> Vec<Span<'static>> {
    // 收集所有匹配及其优先级
    let mut matches: Vec<(usize, usize, Style)> = Vec::new();

    for rule in RULES.iter() {
        for m in rule.regex.find_iter(text) {
            // 继承 base_style 的 bg，确保选中行等背景色不被覆盖
            let style = if let Some(bg) = base_style.bg {
                rule.style.bg(bg)
            } else {
                rule.style
            };
            matches.push((m.start(), m.end(), style));
        }
    }

    if matches.is_empty() {
        return vec![Span::styled(text.to_string(), base_style)];
    }

    // 按起始位置排序，重叠时优先选择先出现的
    matches.sort_by_key(|m| (m.0, m.1));

    // 去重重叠区间（先到先得）
    let mut merged: Vec<(usize, usize, Style)> = Vec::new();
    let mut last_end = 0;
    for (start, end, style) in matches {
        if start >= last_end {
            merged.push((start, end, style));
            last_end = end;
        }
    }

    // 生成 Spans
    let mut spans = Vec::new();
    let mut pos = 0;

    for (start, end, style) in merged {
        if start > pos {
            spans.push(Span::styled(text[pos..start].to_string(), base_style));
        }
        spans.push(Span::styled(text[start..end].to_string(), style));
        pos = end;
    }

    if pos < text.len() {
        spans.push(Span::styled(text[pos..].to_string(), base_style));
    }

    spans
}
