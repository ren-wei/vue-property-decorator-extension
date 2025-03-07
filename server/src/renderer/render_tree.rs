use tokio::fs;
use tower_lsp::lsp_types::Url;
use tracing::error;

use super::{combined_rendered_results, Renderer};

/// # 渲染树
///
/// 初始化过程中的渲染树
///
/// ## 特点
///
/// * 每个节点表示一个文件，每个 vue 文件都在其中
/// * 最下层节点必定是 vue 组件或 vue 组件声明文件
/// * 下层节点继承自上层节点
/// * 同一个 ts 节点可能存在于多个分支中
#[derive(PartialEq, Debug)]
pub struct RenderTree {
    pub roots: Vec<RenderTreeNode>,
}

#[derive(PartialEq, Debug)]
pub struct RenderTreeNode {
    uri: Url,
    cache: InitRenderCache,
    children: Vec<RenderTreeNode>,
}

/// 渲染缓存
#[derive(PartialEq, Debug, Clone)]
pub enum InitRenderCache {
    Unresolved,
    VueResolving(VueResolvingCache),
    TsTransfer(String),
    TsResolved(TsResolvedCache),
    ResolveError,
}

/// vue 组件未渲染完成的缓存
#[derive(PartialEq, Debug, Clone)]
pub struct VueResolvingCache {
    pub script_start_pos: usize,
    pub script_end_pos: usize,
    pub template_compile_result: String,
    pub props: Vec<String>,
    pub render_insert_offset: usize,
    pub source: String,
}

/// vue 组件渲染完成的缓存
#[derive(PartialEq, Debug, Clone)]
pub struct VueResolvedCache {
    props: Vec<String>,
}

/// ts 文件渲染完成的缓存
#[derive(PartialEq, Debug, Clone)]
pub struct TsResolvedCache {
    pub props: Vec<String>,
}

impl PartialOrd for InitRenderCache {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let a = match self {
            InitRenderCache::Unresolved => 0,
            InitRenderCache::VueResolving(_) => 1,
            InitRenderCache::TsTransfer(_) => return None,
            InitRenderCache::TsResolved(_) => 2,
            InitRenderCache::ResolveError => 0,
        };
        let b = match other {
            InitRenderCache::Unresolved => 0,
            InitRenderCache::VueResolving(_) => 1,
            InitRenderCache::TsTransfer(_) => return None,
            InitRenderCache::TsResolved(_) => 2,
            InitRenderCache::ResolveError => 0,
        };
        a.partial_cmp(&b)
    }
}

impl RenderTree {
    pub fn new() -> RenderTree {
        RenderTree { roots: Vec::new() }
    }

    /// 添加节点，如果存在依赖节点，则添加到依赖节点下
    /// 如果添加的节点已存在，则未解析的替换为已解析的
    /// TsTransfer 的节点继承的节点如果也是 TsTransfer，那么它们的应该相等
    pub fn add_node(&mut self, uri: Url, cache: InitRenderCache, extends_uri: Option<Url>) {
        if let Some(extends_uri) = extends_uri {
            // 如果原节点存在，那么从渲染树中取出，并替换缓存，如果不存在，那么创建
            let node = if let Some(mut node) =
                RenderTree::take_node_by_uri(&uri, &cache, &mut self.roots)
            {
                if cache > node.cache {
                    node.cache = cache.clone();
                }
                node
            } else {
                RenderTreeNode {
                    uri,
                    cache: cache.clone(),
                    children: vec![],
                }
            };
            // 将节点挂载到继承节点下，如果继承节点不存在，那么先创建继承节点
            let extends_node = RenderTree::find_node_by_uri(&extends_uri, &cache, &mut self.roots);
            if let Some(extends_node) = extends_node {
                extends_node.children.push(node);
            } else {
                self.roots.push(RenderTreeNode {
                    uri: extends_uri,
                    cache: InitRenderCache::Unresolved,
                    children: vec![node],
                });
            }
        } else {
            if let Some(node) = RenderTree::find_node_by_uri(&uri, &cache, &mut self.roots) {
                if cache > node.cache {
                    node.cache = cache;
                }
            } else {
                self.roots.push(RenderTreeNode {
                    uri,
                    cache,
                    children: vec![],
                });
            }
        }
    }

    /// 获取节点
    /// 如果 uri 是 TsTransfer 节点，那么 cache 是 TsTransfer 并且相等时才返回此节点
    fn find_node_by_uri<'a>(
        uri: &Url,
        cache: &InitRenderCache,
        children: &'a mut Vec<RenderTreeNode>,
    ) -> Option<&'a mut RenderTreeNode> {
        for child in children {
            if child.uri == *uri {
                if let InitRenderCache::TsTransfer(id) = &child.cache {
                    if let InitRenderCache::TsTransfer(cache_id) = cache {
                        if cache_id == id {
                            return Some(child);
                        }
                    }
                } else {
                    return Some(child);
                }
            }
            if let Some(node) = RenderTree::find_node_by_uri(uri, cache, &mut child.children) {
                return Some(node);
            }
        }
        None
    }

    /// 从渲染树中取出节点
    /// 如果 uri 是 TsTransfer 节点，那么 cache 是 TsTransfer 并且相等时才返回此节点
    fn take_node_by_uri(
        uri: &Url,
        cache: &InitRenderCache,
        children: &mut Vec<RenderTreeNode>,
    ) -> Option<RenderTreeNode> {
        let mut index = None;
        for (i, child) in children.iter_mut().enumerate() {
            if child.uri == *uri {
                if let InitRenderCache::TsTransfer(id) = &child.cache {
                    if let InitRenderCache::TsTransfer(cache_id) = cache {
                        if cache_id == id {
                            index = Some(i);
                            break;
                        }
                    }
                } else {
                    index = Some(i);
                    break;
                }
            }
            if let Some(node) = RenderTree::take_node_by_uri(uri, cache, &mut child.children) {
                return Some(node);
            }
        }
        Some(children.remove(index?))
    }
}

impl RenderTreeNode {
    pub fn render(self, mut props: Vec<String>, root_uri: &Url, target_root_uri: &Url) {
        let target_path = Renderer::get_target_path(&self.uri, root_uri, target_root_uri);
        match self.cache {
            InitRenderCache::Unresolved => {
                error!("Unresolved: {}", self.uri.path());
            }
            InitRenderCache::VueResolving(mut cache) => {
                props.append(&mut cache.props);
                let content = combined_rendered_results::combined_rendered_results(
                    cache.script_start_pos,
                    cache.script_end_pos,
                    &cache.template_compile_result,
                    &props,
                    cache.render_insert_offset,
                    &cache.source,
                );
                tokio::spawn(async {
                    fs::write(target_path, content).await.unwrap();
                });
            }
            InitRenderCache::TsTransfer(_) => {}
            InitRenderCache::TsResolved(mut cache) => {
                props.append(&mut cache.props);
            }
            InitRenderCache::ResolveError => {
                error!("ResolveError: {}", self.uri.path());
            }
        }
        for child in self.children {
            child.render(props.clone(), root_uri, target_root_uri);
        }
    }
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::Url;

    use crate::renderer::render_tree::RenderTreeNode;

    use super::{InitRenderCache, RenderTree};

    struct Expected<'a> {
        uri: &'a str,
        children: Vec<Expected<'a>>,
    }

    impl Into<RenderTreeNode> for Expected<'_> {
        fn into(self) -> RenderTreeNode {
            let mut children = vec![];
            for child in self.children {
                children.push(child.into());
            }
            RenderTreeNode {
                uri: Url::from_file_path(self.uri).unwrap(),
                cache: InitRenderCache::Unresolved,
                children,
            }
        }
    }

    fn assert_add_node(
        tree: &mut RenderTree,
        uri: &str,
        extends_uri: Option<&str>,
        expected: Vec<Expected>,
    ) {
        let cache = InitRenderCache::Unresolved;
        tree.add_node(
            Url::from_file_path(uri).unwrap(),
            cache,
            extends_uri.map(|uri| Url::from_file_path(uri).unwrap()),
        );
        let mut roots = vec![];
        for item in expected {
            roots.push(item.into());
        }
        let expected = RenderTree { roots };
        assert_eq!(*tree, expected);
    }

    #[test]
    fn test_extends() {
        let mut tree = RenderTree::new();
        // 测试继承节点
        assert_add_node(
            &mut tree,
            "/tmp/project/one.vue",
            Some("/tmp/project/two.vue"),
            vec![Expected {
                uri: "/tmp/project/two.vue",
                children: vec![Expected {
                    uri: "/tmp/project/one.vue",
                    children: vec![],
                }],
            }],
        );
        // 测试无继承节点
        assert_add_node(
            &mut tree,
            "/tmp/project/three.vue",
            None,
            vec![
                Expected {
                    uri: "/tmp/project/two.vue",
                    children: vec![Expected {
                        uri: "/tmp/project/one.vue",
                        children: vec![],
                    }],
                },
                Expected {
                    uri: "/tmp/project/three.vue",
                    children: vec![],
                },
            ],
        );
        // 测试继承节点已存在
        assert_add_node(
            &mut tree,
            "/tmp/project/four.vue",
            Some("/tmp/project/three.vue"),
            vec![
                Expected {
                    uri: "/tmp/project/two.vue",
                    children: vec![Expected {
                        uri: "/tmp/project/one.vue",
                        children: vec![],
                    }],
                },
                Expected {
                    uri: "/tmp/project/three.vue",
                    children: vec![Expected {
                        uri: "/tmp/project/four.vue",
                        children: vec![],
                    }],
                },
            ],
        );
    }
}
