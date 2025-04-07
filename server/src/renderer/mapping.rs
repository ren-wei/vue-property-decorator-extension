use tower_lsp::lsp_types::{Position, Range, Url};

use super::{render_cache::RenderCache, Renderer};

pub trait Mapping {
    /// 是否是原始文档中的位置
    fn is_position_valid(&self, uri: &Url, position: &Position) -> bool;
    /// 获取位置对应的原始位置，如果位置不在 template_compile_result 中或无效，返回 None
    fn get_original_position(&self, uri: &Url, position: &Position) -> Option<Position>;
    /// 获取范围对应的原始范围，如果范围不在 template_compile_result 中或无效，返回 None
    fn get_original_range(&self, uri: &Url, range: &Range) -> Option<Range>;
    /// 获取文档位置映射后的位置，如果不在 template 表达式范围内，那么返回 None
    fn get_mapping_position(&self, uri: &Url, position: &Position) -> Option<Position>;
    /// 获取 vue 组件所处位置的类型，如果不是 vue 文件或者位置无效，返回 None
    fn get_position_type(&self, uri: &Url, position: &Position) -> Option<PositionType>;
}

/// mapping
impl Mapping for Renderer {
    fn is_position_valid(&self, uri: &Url, position: &Position) -> bool {
        Renderer::is_position_valid_by_document(self.get_document(uri), position)
    }

    fn get_original_position(&self, uri: &Url, position: &Position) -> Option<Position> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            let document = &cache.document;
            let line = document
                .position_at(cache.render_insert_offset as u32 + 1)
                .line
                + 1;
            if line == position.line {
                let offset = cache.template_compile_result.offset_at(Position {
                    line: 0,
                    character: position.character,
                }) as usize;
                let original = self.get_original_offset(uri, offset)? as u32;
                Some(document.position_at(original))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn get_original_range(&self, uri: &Url, range: &Range) -> Option<Range> {
        let start = self.get_original_position(uri, &range.start)?;
        let end = self.get_original_position(uri, &range.end)?;
        Some(Range { start, end })
    }

    fn get_mapping_position(&self, uri: &Url, position: &Position) -> Option<Position> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            let document = &cache.document;
            let offset =
                self.get_mapping_offset(uri, document.offset_at(*position) as usize)? as u32;
            let line = document
                .position_at(cache.render_insert_offset as u32 + 1)
                .line
                + 1;
            Some(Position {
                line,
                character: cache.template_compile_result.position_at(offset).character,
            })
        } else {
            None
        }
    }

    fn get_position_type(&self, uri: &Url, position: &Position) -> Option<PositionType> {
        let cache = &self.render_cache[uri];
        if let RenderCache::VueRenderCache(cache) = cache {
            let offset = cache.document.offset_at(*position) as usize;
            if let Some(template) = &cache.template {
                if template.start < offset && offset < template.end {
                    if let Some(pos) = self.get_mapping_position(uri, position) {
                        return Some(PositionType::TemplateExpr(pos));
                    } else {
                        return Some(PositionType::Template);
                    }
                }
            }
            if let Some(script) = &cache.script {
                if script.start_tag_end.unwrap() < offset && offset < script.end_tag_start.unwrap()
                {
                    return Some(PositionType::Script);
                }
            }
        }
        None
    }
}

impl Renderer {
    /// 获取编译前的偏移量，如果不在 template 范围内，返回 None
    fn get_original_offset(&self, uri: &Url, offset: usize) -> Option<usize> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            if cache.mapping.len() == 0 {
                return None;
            }
            let mut start = 0;
            let mut end = cache.mapping.len();
            while start < end {
                let mid = (start + end) / 2;
                let (target, source, len) = cache.mapping[mid];
                if target + len < offset {
                    if start == mid {
                        start += 1;
                    } else {
                        start = mid;
                    }
                } else if target > offset {
                    end = mid;
                } else {
                    return Some(source + offset - target);
                }
            }
        }
        return None;
    }

    /// 获取编译后的所在的字节位置，如果不在 template 范围内返回 None
    ///
    /// `offset` 是模版上的位置
    fn get_mapping_offset(&self, uri: &Url, offset: usize) -> Option<usize> {
        let cache = self.render_cache.get(uri)?;
        if let RenderCache::VueRenderCache(cache) = cache {
            if cache.mapping.len() == 0 {
                return None;
            }
            let mut start = 0;
            let mut end = cache.mapping.len();
            while start < end {
                let mid = (start + end) / 2;
                let (target, source, len) = cache.mapping[mid];
                if source + len < offset {
                    if start == mid {
                        start += 1;
                    } else {
                        start = mid;
                    }
                } else if source > offset {
                    end = mid;
                } else {
                    return Some(target + offset - source);
                }
            }
        }
        return None;
    }
}

#[derive(PartialEq, Debug)]
pub enum PositionType {
    Script,
    Template,
    TemplateExpr(Position),
}
