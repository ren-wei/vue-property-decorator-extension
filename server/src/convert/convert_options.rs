use tower_lsp::lsp_types::Url;

use crate::renderer::Renderer;

#[derive(Default, Clone)]
pub struct ConvertOptions<'a> {
    pub uri: Option<&'a Url>,
    pub renderer: Option<&'a Renderer>,
}

impl ConvertOptions<'_> {
    pub fn root_uri_target_uri(&self) -> (&Url, &Url) {
        let (root_uri, dest_uri) = self
            .renderer
            .unwrap()
            .root_uri_target_uri()
            .as_ref()
            .unwrap();
        (root_uri, dest_uri)
    }
}
