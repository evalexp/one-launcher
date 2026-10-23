use pinyin::ToPinyinMulti;

/// 为输入的字符串生成全拼与首字母简拼（全部归一化为小写）
pub fn generate_pinyin(input: &str) -> (String, String) {
    let mut full = String::with_capacity(input.len() * 2);
    let mut abbr = String::with_capacity(input.len());

    let mut last_was_space = true;

    for ch in input.chars() {
        if let Some(multi) = ch.to_pinyin_multi() {
            // 中文字符
            if let Some(primary) = multi.into_iter().next() {
                let plain = primary.plain();
                full.push_str(plain);
                if let Some(first_char) = plain.chars().next() {
                    abbr.push(first_char.to_ascii_lowercase());
                }
            }
            last_was_space = false;
        } else if ch.is_alphanumeric() {
            let lower = ch.to_ascii_lowercase();
            full.push(lower);
            // 英文首字母缩写：如果前一个是空格或起始，计入缩写
            if last_was_space {
                abbr.push(lower);
            }
            last_was_space = false;
        } else if ch.is_whitespace() || ch == '-' || ch == '_' {
            full.push(' ');
            last_was_space = true;
        }
    }

    (full, abbr)
}
