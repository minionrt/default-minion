use once_cell::sync::Lazy;
use regex::Regex;

static CODE_FENCE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^```.*$").unwrap());

/// Strip wrapping Markdown code fences
///
/// Many language models are trained to produce code snippets surrounded by Markdown code fences,
/// even when instructed not to.
/// This function tries to detect these code fences and strip them from the text.
/// It uses heuristics to allow actual Markdown content to pass through.
pub fn strip_wrapping_markdown_code_fences(content: &str) -> String {
    let trimmed = content.trim().to_owned();
    let code_fence_count = CODE_FENCE_REGEX.find_iter(&trimmed).count();
    if code_fence_count == 0 {
        // There are no code fences in the text.
        return content.to_owned();
    }
    // Strip the first code fence
    let mut start_idx = 0;
    if let Some(first_line) = trimmed.lines().next() {
        if CODE_FENCE_REGEX.is_match(first_line) {
            start_idx = first_line.len() + 1;
        }
    }

    // Strip the last code fence under two conditions:
    // * There is an odd number of code fences. This indicates invalid Markdown.
    // * There was a code fence at the beginning. This indicates the whole content is wrapped in code fences.
    let mut end_idx = trimmed.len();
    if code_fence_count % 2 == 1 || start_idx != 0 {
        if let Some(last_line) = trimmed.lines().last() {
            if CODE_FENCE_REGEX.is_match(last_line) {
                end_idx -= last_line.len();
            }
        }
    }

    // Ensure indices are within bounds
    // The start index should be at most the length of the content
    start_idx = start_idx.min(trimmed.len());
    // The end index should be at least the start index
    end_idx = end_idx.max(start_idx);

    let has_been_stripped = start_idx != 0 || end_idx != trimmed.len();

    if has_been_stripped {
        trimmed[start_idx..end_idx].to_owned()
    } else {
        // If nothing has been stripped, return the original, untrimmed content
        content.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_wrapped_code_fence() {
        let input = "```python\nprint(\"Hello World\")\n```";
        let expected = "print(\"Hello World\")\n";
        assert_eq!(strip_wrapping_markdown_code_fences(input), expected);
    }

    #[test]
    fn test_strip_with_inner_code_fence() {
        let input = "Some text\n\n```python\nprint(\"Hello World\")\n```\n\nSome more text";
        let expected = input;
        assert_eq!(strip_wrapping_markdown_code_fences(input), expected);
    }

    #[test]
    fn test_no_code_fence() {
        let input = "This is some text without code fences.";
        let expected = input;
        assert_eq!(strip_wrapping_markdown_code_fences(input), expected);
    }

    #[test]
    fn test_only_start_fence() {
        let input = "```\nSome code without ending fence";
        let expected = "Some code without ending fence";
        assert_eq!(strip_wrapping_markdown_code_fences(input), expected);
    }

    #[test]
    fn test_only_end_fence() {
        let input = "Some code without starting fence\n```";
        let expected = "Some code without starting fence\n";
        assert_eq!(strip_wrapping_markdown_code_fences(input), expected);
    }

    #[test]
    fn test_wrapped_with_extra_spaces() {
        let input = "   ```\n   code\n```   ";
        let expected = "   code\n";
        assert_eq!(strip_wrapping_markdown_code_fences(input), expected);
    }

    #[test]
    fn test_empty_input() {
        let input = "";
        let expected = "";
        assert_eq!(strip_wrapping_markdown_code_fences(input), expected);
    }

    #[test]
    fn test_only_two_code_fences() {
        let input = "```\n```";
        let expected = "";
        assert_eq!(strip_wrapping_markdown_code_fences(input), expected);
    }

    #[test]
    fn test_only_one_code_fence() {
        let input = "```";
        let expected = "";
        assert_eq!(strip_wrapping_markdown_code_fences(input), expected);
    }

    #[test]
    fn test_proper_markdown() {
        let input = r#"
# Hello

```rust
fn foo() {}
```
"#;
        let expected = input;
        assert_eq!(strip_wrapping_markdown_code_fences(input), expected);
    }

    #[test]
    fn test_markdown_wrapped_in_code_fences() {
        let input = r#"
```

# Hello

```rust
fn foo() {}
```

```
"#;
        let expected = r#"
# Hello

```rust
fn foo() {}
```

"#;
        assert_eq!(strip_wrapping_markdown_code_fences(input), expected);
    }
}
