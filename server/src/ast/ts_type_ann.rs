use swc_ecma_ast::{TsType, TsTypeAnn};

pub fn get_ts_type_string(ts_type: &Option<Box<TsTypeAnn>>) -> String {
    if let Some(ts_type) = ts_type {
        match ts_type.type_ann.as_ref() {
            TsType::TsKeywordType(ts_type) => {
                if let Ok(kind) = serde_json::to_string(&ts_type.kind) {
                    (&kind[1..kind.len() - 1]).to_string()
                } else {
                    "unknown".to_string()
                }
            }
            _ => "unknown".to_string(),
        }
    } else {
        "unknown".to_string()
    }
}
