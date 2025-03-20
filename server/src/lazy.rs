use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    pub(crate) static ref REG_DOUBLE_BRACES: Regex = Regex::new(r"\{\{(.*?)\}\}").unwrap();
    pub(crate) static ref REG_TYPESCRIPT_MODULE: Regex =
        Regex::new(r#"(?s)^\n```typescript\nmodule "(.*)"\n```\n$"#).unwrap();
    pub(crate) static ref REG_V_FOR_WITH_INDEX: Regex = Regex::new(r"\((\w+),\s*(\w+)\)").unwrap();
    pub(crate) static ref REG_SINGLE_BRACKET: Regex =
        Regex::new(r"\{[^\s}][^}]*|\}[^{]*[^\s{]").unwrap();
}
