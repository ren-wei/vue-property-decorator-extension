use std::collections::HashMap;

use html_languageservice::parser::html_document::Node;
use lsp_textdocument::FullTextDocument;
use petgraph::{graph::NodeIndex, visit::EdgeRef, Direction, Graph};
use swc_common::util::take::Take;
use tokio::fs;
use tower_lsp::lsp_types::Url;

use super::{combined_rendered_results, template_compile::CompileMapping, Renderer};

type RRGraph = Graph<RenderCache, Relationship>;

/// 存储组件渲染缓存和组件间关系的图
pub struct RenderCacheGraph {
    graph: RRGraph,
    idx_map: HashMap<Url, NodeIndex>,
    /// 未加入的边
    virtual_edges: Vec<(Url, Url, Relationship)>,
}

impl RenderCacheGraph {
    pub fn new() -> Self {
        RenderCacheGraph {
            graph: Graph::new(),
            idx_map: HashMap::new(),
            virtual_edges: vec![],
        }
    }

    pub fn get(&self, uri: &Url) -> Option<&RenderCache> {
        let idx = self.idx_map.get(uri)?;
        self.graph.node_weight(*idx)
    }

    pub fn get_mut(&mut self, uri: &Url) -> Option<&mut RenderCache> {
        let idx = self.idx_map.get(uri)?;
        self.graph.node_weight_mut(*idx)
    }

    /// 如果节点不存在，那么直接新增，如果节点存在那么更新缓存
    pub fn add_node(&mut self, uri: &Url, cache: RenderCache) {
        // 检查对应节点是否存在
        let idx = self.idx_map.get(uri);
        if let Some(idx) = idx {
            let node = self.graph.node_weight_mut(*idx).unwrap();
            *node = cache;
        } else {
            let idx = self.graph.add_node(cache);
            self.idx_map.insert(uri.clone(), idx);
        }
    }

    /// 添加边，如果存在相同的边，那么忽略
    ///
    /// *Panics* 如果节点不存在
    pub fn add_edge(&mut self, from: &Url, to: &Url, relation: Relationship) {
        let a = *self.idx_map.get(from).unwrap();
        let b = *self.idx_map.get(to).unwrap();
        // 检查相同的边是否存在
        let mut edges = self.graph.edges_connecting(a, b);
        if edges.find(|edge| *edge.weight() == relation).is_none() {
            self.graph.add_edge(a, b, relation);
        }
    }

    /// 添加虚拟边，不实际添加入 graph 避免节点不存在出现 panic
    /// 当所以节点都被添加后，请使用 flush 将所有边加入 graph
    pub fn add_virtual_edge(&mut self, from: &Url, to: &Url, relation: Relationship) {
        self.virtual_edges
            .push((from.clone(), to.clone(), relation));
    }

    /// 移除节点下游边
    pub fn remove_outgoing_edge(&mut self, uri: &Url) {
        let idx = *self.idx_map.get(uri).unwrap();
        let edges = self
            .graph
            .edges_directed(idx, Direction::Outgoing)
            .map(|v| v.id())
            .collect::<Vec<_>>();
        for edge in edges {
            self.graph.remove_edge(edge);
        }
    }

    /// 将所有虚拟边加入 graph
    pub fn flush(&mut self) {
        for (from, to, relation) in self.virtual_edges.take() {
            self.add_edge(&from, &to, relation);
        }
    }

    /// 渲染到文件系统
    pub fn render(&self, root_uri: &Url, target_root_uri: &Url) {
        for node in self.graph.node_indices() {
            let cache = &self.graph[node];
            if let RenderCache::VueRenderCache(_) = cache {
                let uri = self.get_node_uri(node);
                let content = self.get_node_render_content(uri).unwrap();
                let target_path = Renderer::get_target_path(uri, root_uri, target_root_uri);
                tokio::spawn(async {
                    fs::write(target_path, content).await.unwrap();
                });
            }
        }
    }

    /// 获取节点渲染内容
    /// 如果是 vue 节点，那么获取渲染后的内容
    /// 如果是 ts 节点，那么返回 None
    pub fn get_node_render_content(&self, uri: &Url) -> Option<String> {
        let node = *self.idx_map.get(uri).unwrap();
        let cache = &self.graph[node];
        if let RenderCache::VueRenderCache(cache) = cache {
            // 获取继承组件的 props
            let mut props = RenderCacheGraph::get_extends_props(&self.graph, node);
            props.append(&mut cache.props.clone());
            Some(combined_rendered_results::combined_rendered_results(
                cache.script.start_tag_end.unwrap(),
                cache.script.end_tag_start.unwrap(),
                &cache.template_compile_result,
                &props,
                cache.render_insert_offset,
                cache.document.get_content(None),
            ))
        } else {
            None
        }
    }

    /// 根据索引反向查找 uri
    fn get_node_uri(&self, idx: NodeIndex) -> &Url {
        for (key, value) in &self.idx_map {
            if *value == idx {
                return key;
            }
        }
        panic!("get_node_uri not found");
    }

    /// 获取当前节点的所有继承属性
    fn get_extends_props(graph: &RRGraph, node: NodeIndex) -> Vec<String> {
        let mut extends_props = vec![];
        let mut next_node = RenderCacheGraph::get_extends_node(&graph, node);
        while let Some((cur_node, export_name)) = next_node {
            match &graph[cur_node] {
                RenderCache::VueRenderCache(cache) => {
                    extends_props.append(&mut cache.props.clone());
                    next_node = RenderCacheGraph::get_extends_node(&graph, cur_node);
                }
                RenderCache::TsRenderCache(cache) => {
                    // 尝试从当前文件获取下一个节点
                    if let Some(ts_component) = &cache.ts_component {
                        if export_name == None {
                            extends_props.append(&mut ts_component.props.clone());
                            next_node = RenderCacheGraph::get_extends_node(&graph, cur_node);
                            continue;
                        } else if cache.local_exports.contains(&export_name) {
                            // 从当前定义，但是不是组件，那么直接退出
                            break;
                        }
                    }
                    // 尝试从转换关系获取下一个节点
                    if let Some((transfer_node, export_name)) =
                        RenderCacheGraph::get_transfer_node(&graph, cur_node, &export_name)
                    {
                        next_node = Some((transfer_node, export_name));
                        continue;
                    }
                    // 尝试从星号导出获取下一个节点
                    if let Some((node, export_name)) =
                        RenderCacheGraph::get_node_from_star_export(&graph, cur_node, &export_name)
                    {
                        next_node = Some((node, export_name));
                    } else {
                        // 未找到
                        break;
                    }
                }
                RenderCache::Unknown => {
                    next_node = None;
                }
            }
        }
        extends_props
    }

    /// 获取继承的节点
    fn get_extends_node(graph: &RRGraph, node: NodeIndex) -> Option<(NodeIndex, Option<String>)> {
        let mut edges = graph.edges_directed(node, Direction::Outgoing);
        let extends_edge = edges.find(|edge| edge.weight().is_extends())?;
        let export_name = extends_edge.weight().as_extends().export_name.clone();
        let extends_node = extends_edge.target();
        Some((extends_node, export_name))
    }

    /// 从转换关系获取节点
    fn get_transfer_node(
        graph: &RRGraph,
        node: NodeIndex,
        export_name: &Option<String>,
    ) -> Option<(NodeIndex, Option<String>)> {
        let edges = graph.edges_directed(node, Direction::Outgoing);
        for edge in edges {
            if let Relationship::TransferRelationship(relation) = edge.weight() {
                if &relation.local == export_name {
                    return Some((edge.target(), relation.export_name.clone()));
                }
            }
        }
        None
    }

    /// 从星号导出获取节点
    fn get_node_from_star_export(
        graph: &RRGraph,
        node: NodeIndex,
        export_name: &Option<String>,
    ) -> Option<(NodeIndex, Option<String>)> {
        // TODO: 从星号导出获取节点
        None
    }
}

pub enum RenderCache {
    VueRenderCache(VueRenderCache),
    TsRenderCache(TsRenderCache),
    Unknown,
}

/// vue 组件的渲染缓存
pub struct VueRenderCache {
    /// 渲染前的文档，与文件系统中相同
    pub document: FullTextDocument,
    // 解析文档
    pub template: Node,
    pub script: Node,
    pub style: Vec<Node>,
    // 解析模版
    pub template_compile_result: String,
    pub mapping: CompileMapping,
    /// 解析脚本得到的属性
    pub props: Vec<String>,
    pub render_insert_offset: usize,
}

/// ts 文件的渲染缓存
pub struct TsRenderCache {
    /// ts 文件中定义的组件
    pub ts_component: Option<TsComponent>,
    /// 从当前文件定义并导出的名称
    pub local_exports: Vec<Option<String>>,
}

pub struct TsComponent {
    pub props: Vec<String>,
}

#[derive(PartialEq)]
pub enum Relationship {
    ExtendsRelationship(ExtendsRelationship),
    RegisterRelationship(RegisterRelationship),
    TransferRelationship(TransferRelationship),
}

impl Relationship {
    pub fn is_extends(&self) -> bool {
        if let Relationship::ExtendsRelationship(_) = self {
            true
        } else {
            false
        }
    }

    pub fn as_extends(&self) -> &ExtendsRelationship {
        if let Relationship::ExtendsRelationship(relation) = self {
            relation
        } else {
            panic!("Relationship as_extends but it's not ExtendsRelationship");
        }
    }
}

/// 节点间的继承关系，指向被继承的节点
#[derive(PartialEq)]
pub struct ExtendsRelationship {
    pub export_name: Option<String>,
}

/// 节点间的注册关系，指向被注册的节点
#[derive(PartialEq)]
pub struct RegisterRelationship {
    /// 注册的名称
    pub registered_name: String,
    /// 导出的名称
    pub export_name: Option<String>,
    /// `导出的名称的属性`
    /// 如果是使用类似 Select.Option 注册的，
    /// 那么 prop 是 Some("Option"), export_name 是 Some("Select")，
    pub prop: Option<String>,
}

/// 节点间的中转关系，指向导入的节点
#[derive(PartialEq)]
pub struct TransferRelationship {
    /// 当前文件导出时的名称
    pub local: Option<String>,
    /// 从其他组件中导出的名称
    pub export_name: Option<String>,
    /// 是否是 * 导出
    /// 如果是，那么
    /// * 形如 `export * from "xxx"` 的关系表示为 local 和 export_name 为 None
    /// * 形如 `export * as OtherName from "xxx"` 的关系表示为 local 为 Some("OtherName")
    /// * 形如 `export * as default from "xxx"` 的关系表示为 local 为 Some("default")
    pub is_star_export: bool,
}
