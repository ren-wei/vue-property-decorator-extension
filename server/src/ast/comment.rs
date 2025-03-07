use regex::Regex;
use swc_common::comments::Comment;

pub fn get_markdown(comment: &Comment) -> String {
    let mut text: &str = &comment.text.to_string();
    if text.chars().next() == Some('*') {
        text = &text[1..];
    }
    // 移除前面的 * 号
    let re = Regex::new(r"\n\s+\*").unwrap();
    let result = re.replace_all(&text, "\n\n").to_string();
    // 给参数注释增加样式
    let re = Regex::new(r"(@\w+)\s([a-zA-Z_][a-zA-Z0-9_]+)\s").unwrap();
    let result = re.replace_all(&result, "*$1* `$2` ").to_string();
    result
}
