/// 组合渲染结果
pub fn combined_rendered_results(
    script_start_pos: usize,
    script_end_pos: usize,
    template_compile_result: &str,
    props: &Vec<String>,
    render_insert_offset: usize,
    source: &str,
) -> String {
    let source = get_fill_space_source(source, script_start_pos, script_end_pos);
    format!(
        "{}protected render(){{let {{{}}} = this;const $event:any;\n{}{}",
        &source[..render_insert_offset],
        props.join(","),
        template_compile_result,
        &source[render_insert_offset..]
    )
}

/// 将指定范围之外的部分填充空白
fn get_fill_space_source(source: &str, start_pos: usize, end_pos: usize) -> String {
    let mut char_iter = source.bytes().peekable();
    let mut result = vec![];
    let mut idx = 0;

    while let Some(ch) = char_iter.next() {
        if idx >= start_pos && idx < end_pos {
            result.push(ch);
        } else {
            result.push(b' ');
        }

        // 跳过字符串中的换行符，但将它们添加到结果中
        while let Some(next_ch) = char_iter.peek() {
            if *next_ch == b'\n' {
                result.push(b'\n');
                char_iter.next(); // 消耗换行符
                idx += 1;
            } else {
                break;
            }
        }
        idx += 1;
    }

    String::from_utf8(result).unwrap()
}
